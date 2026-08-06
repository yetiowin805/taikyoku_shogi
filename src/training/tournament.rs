//! Round-robin tournament with Elo, checkpoint/resume, and cooperative stop.

use crate::game_history::GameResult;
use crate::training::paths::ensure_data_dirs;
use crate::training::pool::parse_starts_spec;
use crate::training::record::AgentSpec;
use crate::training::worker::{play_one_game, WorkerConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_ELO: f64 = 1500.0;
pub const DEFAULT_ELO_K: f64 = 20.0;
pub const DEFAULT_GAMES_PER_PAIR: usize = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    Pending,
    Running,
    Done,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourneyEntrant {
    pub id: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourneySlot {
    pub id: usize,
    pub model_a: String,
    pub model_b: String,
    pub start_seed: u64,
    /// When true, `model_a` plays Black.
    pub a_is_black: bool,
    pub status: SlotStatus,
    #[serde(default)]
    pub game_path: Option<String>,
    /// 1.0 = A won, 0.0 = B won, 0.5 = draw (from A's perspective).
    #[serde(default)]
    pub score_a: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourneyState {
    pub run_id: String,
    pub depth: u32,
    pub starts_spec: String,
    pub seed_base: u64,
    pub games_per_pair: usize,
    pub entrants: Vec<TourneyEntrant>,
    pub slots: Vec<TourneySlot>,
    pub elo: BTreeMap<String, f64>,
    pub elo_k: f64,
    pub updated_at: u64,
}

#[derive(Debug, Clone)]
pub struct TourneyConfig {
    pub run_id: String,
    pub outdir: PathBuf,
    pub entrants: Vec<TourneyEntrant>,
    pub starts_spec: String,
    pub depth: u32,
    pub games_per_pair: usize,
    pub jobs: usize,
    pub seed_base: u64,
    pub max_moves: usize,
    pub elo_k: f64,
    pub stop_file: PathBuf,
    pub resume: bool,
    pub verbose: bool,
    pub stop: Arc<AtomicBool>,
}

impl Default for TourneyConfig {
    fn default() -> Self {
        Self {
            run_id: new_run_id(),
            outdir: PathBuf::from("data/raw/tourney"),
            entrants: Vec::new(),
            starts_spec: "light".into(),
            depth: 2,
            games_per_pair: DEFAULT_GAMES_PER_PAIR,
            jobs: 1,
            seed_base: 1,
            max_moves: crate::training::worker::DEFAULT_MAX_MOVES,
            elo_k: DEFAULT_ELO_K,
            stop_file: PathBuf::from("data/run/TOURNEY_STOP"),
            resume: false,
            verbose: false,
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn new_run_id() -> String {
    format!("tourney-{}", now_secs())
}

fn expected_score(ra: f64, rb: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf((rb - ra) / 400.0))
}

pub fn elo_update(ra: f64, rb: f64, score_a: f64, k: f64) -> (f64, f64) {
    let ea = expected_score(ra, rb);
    let eb = 1.0 - ea;
    (ra + k * (score_a - ea), rb + k * ((1.0 - score_a) - eb))
}

fn score_from_result(a_is_black: bool, result: &Option<GameResult>) -> f64 {
    match result {
        Some(GameResult::Draw) | None => 0.5,
        Some(GameResult::BlackWins) => {
            if a_is_black {
                1.0
            } else {
                0.0
            }
        }
        Some(GameResult::WhiteWins) => {
            if a_is_black {
                0.0
            } else {
                1.0
            }
        }
    }
}

fn build_schedule(cfg: &TourneyConfig) -> TourneyState {
    let mut elo = BTreeMap::new();
    for e in &cfg.entrants {
        elo.insert(e.id.clone(), DEFAULT_ELO);
    }
    // Interleave by round so every matchup stays roughly even if you stop early:
    // round g gives each unordered pair one start (both colors) before round g+1.
    let mut pairings = Vec::new();
    let n = cfg.entrants.len();
    for i in 0..n {
        for j in (i + 1)..n {
            pairings.push((cfg.entrants[i].id.clone(), cfg.entrants[j].id.clone()));
        }
    }
    let mut slots = Vec::new();
    let mut id = 0usize;
    for g in 0..cfg.games_per_pair {
        for (pi, (model_a, model_b)) in pairings.iter().enumerate() {
            let start_seed = cfg
                .seed_base
                .wrapping_add((g as u64).wrapping_mul(1_000_003))
                .wrapping_add((pi as u64).wrapping_mul(17));
            for a_is_black in [true, false] {
                slots.push(TourneySlot {
                    id,
                    model_a: model_a.clone(),
                    model_b: model_b.clone(),
                    start_seed,
                    a_is_black,
                    status: SlotStatus::Pending,
                    game_path: None,
                    score_a: None,
                });
                id += 1;
            }
        }
    }
    TourneyState {
        run_id: cfg.run_id.clone(),
        depth: cfg.depth,
        starts_spec: cfg.starts_spec.clone(),
        seed_base: cfg.seed_base,
        games_per_pair: cfg.games_per_pair,
        entrants: cfg.entrants.clone(),
        slots,
        elo,
        elo_k: cfg.elo_k,
        updated_at: now_secs(),
    }
}

fn run_dir(cfg: &TourneyConfig) -> PathBuf {
    cfg.outdir.join(&cfg.run_id)
}

fn state_path(cfg: &TourneyConfig) -> PathBuf {
    run_dir(cfg).join("state.json")
}

fn standings_path(cfg: &TourneyConfig) -> PathBuf {
    run_dir(cfg).join("standings.md")
}

fn elo_path(cfg: &TourneyConfig) -> PathBuf {
    run_dir(cfg).join("elo.json")
}

pub fn load_state(path: &Path) -> Result<TourneyState, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn save_state(cfg: &TourneyConfig, state: &TourneyState) -> Result<(), String> {
    let dir = run_dir(cfg);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = state_path(cfg);
    let json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    let elo_json = serde_json::to_string_pretty(&state.elo).map_err(|e| e.to_string())?;
    fs::write(elo_path(cfg), elo_json).map_err(|e| e.to_string())?;
    fs::write(standings_path(cfg), format_standings(state))
        .map_err(|e| format!("write standings: {e}"))?;
    Ok(())
}

pub fn format_standings(state: &TourneyState) -> String {
    let mut rows: Vec<_> = state.elo.iter().collect();
    rows.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    let done = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Done)
        .count();
    let total = state.slots.len();
    let mut out = String::new();
    out.push_str(&format!(
        "# Tournament {} standings\n\nGames finished: {done}/{total}\n\n| Rank | Model | Elo |\n|---:|---|---:|\n",
        state.run_id
    ));
    for (i, (id, elo)) in rows.iter().enumerate() {
        out.push_str(&format!("| {} | {} | {:.1} |\n", i + 1, id, *elo));
    }
    out.push('\n');
    out
}

pub fn standings_summary(state: &TourneyState) -> String {
    let mut rows: Vec<_> = state.elo.iter().collect();
    rows.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
    let done = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Done)
        .count();
    let mut lines = vec![format!(
        "run={} games={}/{}",
        state.run_id,
        done,
        state.slots.len()
    )];
    for (id, elo) in rows {
        let delta = elo - DEFAULT_ELO;
        lines.push(format!("{id}: {elo:.1} (Δ{delta:+.1})"));
    }
    lines.join("\n")
}

fn model_path_for<'a>(state: &'a TourneyState, id: &str) -> Result<&'a str, String> {
    state
        .entrants
        .iter()
        .find(|e| e.id == id)
        .map(|e| e.model.as_str())
        .ok_or_else(|| format!("unknown entrant {id}"))
}

fn agent_for(state: &TourneyState, id: &str, depth: u32) -> Result<AgentSpec, String> {
    let mut a = AgentSpec::new("ab");
    a.depth = Some(depth);
    a.model = Some(model_path_for(state, id)?.to_string());
    Ok(a)
}

fn poll_stop_file(cfg: &TourneyConfig) {
    if cfg.stop_file.exists() {
        cfg.stop.store(true, Ordering::Relaxed);
    }
}

/// Run or resume a tournament. Returns final state (possibly partial on stop).
pub fn run_tournament(cfg: &TourneyConfig) -> Result<TourneyState, String> {
    ensure_data_dirs()?;
    let dir = run_dir(cfg);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let _ = fs::remove_file(&cfg.stop_file);

    let mut state = if cfg.resume && state_path(cfg).exists() {
        let mut s = load_state(&state_path(cfg))?;
        for slot in &mut s.slots {
            if slot.status == SlotStatus::Running || slot.status == SlotStatus::Aborted {
                slot.status = SlotStatus::Pending;
                slot.game_path = None;
                slot.score_a = None;
            }
        }
        s
    } else {
        if cfg.entrants.len() < 2 {
            return Err("tournament needs at least 2 entrants".into());
        }
        build_schedule(cfg)
    };
    save_state(cfg, &state)?;

    let starts = parse_starts_spec(&cfg.starts_spec)?;
    let jobs = cfg.jobs.max(1);
    let state_mu = Arc::new(Mutex::new(state));
    let cfg_stop = cfg.stop.clone();
    let games_dir = dir.clone();

    // Ctrl-C → stop flag
    let stop_for_handler = cfg_stop.clone();
    let _ = ctrlc::set_handler(move || {
        stop_for_handler.store(true, Ordering::Relaxed);
        eprintln!("tournament: stop requested (will abort in-flight games)");
    });

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let state_mu = Arc::clone(&state_mu);
            let cfg_stop = Arc::clone(&cfg_stop);
            let starts = &starts;
            let games_dir = &games_dir;
            scope.spawn(move || loop {
                poll_stop_file(cfg);
                if cfg_stop.load(Ordering::Relaxed) {
                    return;
                }

                let slot_id = {
                    let mut st = state_mu.lock().unwrap();
                    let found = st
                        .slots
                        .iter()
                        .position(|s| s.status == SlotStatus::Pending);
                    let Some(idx) = found else {
                        return;
                    };
                    st.slots[idx].status = SlotStatus::Running;
                    st.updated_at = now_secs();
                    let id = st.slots[idx].id;
                    let _ = save_state(cfg, &st);
                    id
                };

                let (model_a, model_b, start_seed, a_is_black, depth) = {
                    let st = state_mu.lock().unwrap();
                    let slot = &st.slots[slot_id];
                    (
                        slot.model_a.clone(),
                        slot.model_b.clone(),
                        slot.start_seed,
                        slot.a_is_black,
                        st.depth,
                    )
                };

                if cfg_stop.load(Ordering::Relaxed) {
                    let mut st = state_mu.lock().unwrap();
                    st.slots[slot_id].status = SlotStatus::Aborted;
                    let _ = save_state(cfg, &st);
                    return;
                }

                let start = match starts.start_for_seed(start_seed) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("tourney start error: {e}");
                        let mut st = state_mu.lock().unwrap();
                        st.slots[slot_id].status = SlotStatus::Aborted;
                        let _ = save_state(cfg, &st);
                        return;
                    }
                };

                let (black, white) = {
                    let st = state_mu.lock().unwrap();
                    let a = match agent_for(&st, &model_a, depth) {
                        Ok(a) => a,
                        Err(e) => {
                            eprintln!("{e}");
                            return;
                        }
                    };
                    let b = match agent_for(&st, &model_b, depth) {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("{e}");
                            return;
                        }
                    };
                    if a_is_black {
                        (a, b)
                    } else {
                        (b, a)
                    }
                };

                let play_seed = start_seed.wrapping_add(if a_is_black { 0 } else { 1 });
                match play_one_game(&WorkerConfig {
                    black,
                    white,
                    start,
                    seed: play_seed,
                    max_moves: cfg.max_moves,
                    verbose: cfg.verbose && jobs == 1,
                    stop: Some(Arc::clone(&cfg_stop)),
                }) {
                    Ok(rec) => {
                        let path = games_dir.join(format!(
                            "slot{:04}-{}-vs-{}-{}.json",
                            slot_id,
                            model_a,
                            model_b,
                            if a_is_black { "a-black" } else { "a-white" }
                        ));
                        let _ = rec.save_path(&path);
                        let score_a = score_from_result(a_is_black, &rec.result);
                        let mut st = state_mu.lock().unwrap();
                        let ra = *st.elo.get(&model_a).unwrap_or(&DEFAULT_ELO);
                        let rb = *st.elo.get(&model_b).unwrap_or(&DEFAULT_ELO);
                        let (na, nb) = elo_update(ra, rb, score_a, st.elo_k);
                        st.elo.insert(model_a.clone(), na);
                        st.elo.insert(model_b.clone(), nb);
                        st.slots[slot_id].status = SlotStatus::Done;
                        st.slots[slot_id].score_a = Some(score_a);
                        st.slots[slot_id].game_path = Some(path.display().to_string());
                        st.updated_at = now_secs();
                        let _ = save_state(cfg, &st);
                        if cfg.verbose {
                            println!(
                                "slot {slot_id} done {} vs {} score_a={score_a:.1}",
                                model_a, model_b
                            );
                        }
                    }
                    Err(e) => {
                        let mut st = state_mu.lock().unwrap();
                        st.slots[slot_id].status = SlotStatus::Aborted;
                        st.updated_at = now_secs();
                        let _ = save_state(cfg, &st);
                        if e.message != "stopped" {
                            eprintln!("slot {slot_id} failed: {}", e.message);
                        }
                        if cfg_stop.load(Ordering::Relaxed) {
                            return;
                        }
                    }
                }
            });
        }
    });

    // On stop, mark leftover Running as Aborted.
    let mut state = state_mu.lock().unwrap().clone();
    for slot in &mut state.slots {
        if slot.status == SlotStatus::Running {
            slot.status = SlotStatus::Aborted;
        }
    }
    state.updated_at = now_secs();
    save_state(cfg, &state)?;
    println!("{}", format_standings(&state));
    Ok(state)
}

/// Load entrants from a tourney manifest JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TourneyManifest {
    pub entrants: Vec<TourneyEntrant>,
}

pub fn load_manifest(path: &Path) -> Result<TourneyManifest, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&s).map_err(|e| format!("parse {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elo_favors_winner() {
        let (a, b) = elo_update(1500.0, 1500.0, 1.0, 20.0);
        assert!(a > 1500.0);
        assert!(b < 1500.0);
    }

    #[test]
    fn schedule_size() {
        let cfg = TourneyConfig {
            entrants: vec![
                TourneyEntrant {
                    id: "a".into(),
                    model: "a.json".into(),
                },
                TourneyEntrant {
                    id: "b".into(),
                    model: "b.json".into(),
                },
                TourneyEntrant {
                    id: "c".into(),
                    model: "c.json".into(),
                },
            ],
            games_per_pair: 2,
            ..TourneyConfig::default()
        };
        let st = build_schedule(&cfg);
        // C(3,2)=3 pairings * 2 rounds * 2 colors = 12
        assert_eq!(st.slots.len(), 12);
    }

    #[test]
    fn schedule_interleaves_matchups_each_round() {
        let cfg = TourneyConfig {
            entrants: vec![
                TourneyEntrant {
                    id: "a".into(),
                    model: "a.json".into(),
                },
                TourneyEntrant {
                    id: "b".into(),
                    model: "b.json".into(),
                },
                TourneyEntrant {
                    id: "c".into(),
                    model: "c.json".into(),
                },
            ],
            games_per_pair: 3,
            ..TourneyConfig::default()
        };
        let st = build_schedule(&cfg);
        let first_round: Vec<_> = st
            .slots
            .iter()
            .take(6)
            .map(|s| (s.model_a.as_str(), s.model_b.as_str()))
            .collect();
        assert_eq!(first_round[0], ("a", "b"));
        assert_eq!(first_round[2], ("a", "c"));
        assert_eq!(first_round[4], ("b", "c"));
        // Second round starts at index 6 with a-b again.
        assert_eq!(st.slots[6].model_a, "a");
        assert_eq!(st.slots[6].model_b, "b");
        // Color-swapped pair shares start_seed.
        assert_eq!(st.slots[0].start_seed, st.slots[1].start_seed);
        assert_ne!(st.slots[0].a_is_black, st.slots[1].a_is_black);
    }
}

#[cfg(test)]
mod start_seed_tests {
    use super::*;
    use crate::training::pool::parse_starts_spec;
    use crate::training::record::GameStart;

    #[test]
    fn color_pair_shares_identical_light_start() {
        let src = parse_starts_spec("light").unwrap();
        let seed = 42u64;
        let a = src.start_for_seed(seed).unwrap();
        let b = src.start_for_seed(seed).unwrap();
        match (a, b) {
            (GameStart::Position { position: pa }, GameStart::Position { position: pb }) => {
                assert_eq!(pa, pb);
            }
            _ => panic!("expected Position"),
        }
    }
}
