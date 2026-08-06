//! Round-robin / Swiss tournament with Elo, checkpoint/resume, and cooperative stop.

use crate::game_history::GameResult;
use crate::training::paths::ensure_data_dirs;
use crate::training::pool::parse_starts_spec;
use crate::training::record::AgentSpec;
use crate::training::worker::{play_one_game, WorkerConfig};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_ELO: f64 = 1500.0;
pub const DEFAULT_ELO_K: f64 = 20.0;
pub const DEFAULT_GAMES_PER_PAIR: usize = 24;
pub const DEFAULT_SWISS_ROUNDS: usize = 5;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TourneyFormat {
    #[default]
    RoundRobin,
    Swiss,
}

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
    /// Swiss round index (0-based). Round-robin leaves this at 0.
    #[serde(default)]
    pub round: usize,
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
    #[serde(default)]
    pub format: TourneyFormat,
    #[serde(default)]
    pub swiss_rounds: usize,
    /// Highest Swiss round index that has been scheduled (0-based). RR unused.
    #[serde(default)]
    pub swiss_next_round: usize,
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
    pub format: TourneyFormat,
    pub swiss_rounds: usize,
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
            format: TourneyFormat::RoundRobin,
            swiss_rounds: DEFAULT_SWISS_ROUNDS,
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
    let mut state = TourneyState {
        run_id: cfg.run_id.clone(),
        depth: cfg.depth,
        starts_spec: cfg.starts_spec.clone(),
        seed_base: cfg.seed_base,
        games_per_pair: cfg.games_per_pair,
        entrants: cfg.entrants.clone(),
        slots: Vec::new(),
        elo,
        elo_k: cfg.elo_k,
        updated_at: now_secs(),
        format: cfg.format,
        swiss_rounds: cfg.swiss_rounds,
        swiss_next_round: 0,
    };
    match cfg.format {
        TourneyFormat::RoundRobin => {
            // Interleave by round so every matchup stays roughly even if you stop early:
            // round g gives each unordered pair one start (both colors) before round g+1.
            let mut pairings = Vec::new();
            let n = cfg.entrants.len();
            for i in 0..n {
                for j in (i + 1)..n {
                    pairings.push((cfg.entrants[i].id.clone(), cfg.entrants[j].id.clone()));
                }
            }
            let mut id = 0usize;
            for g in 0..cfg.games_per_pair {
                for (pi, (model_a, model_b)) in pairings.iter().enumerate() {
                    let start_seed = cfg
                        .seed_base
                        .wrapping_add((g as u64).wrapping_mul(1_000_003))
                        .wrapping_add((pi as u64).wrapping_mul(17));
                    for a_is_black in [true, false] {
                        state.slots.push(TourneySlot {
                            id,
                            model_a: model_a.clone(),
                            model_b: model_b.clone(),
                            start_seed,
                            a_is_black,
                            status: SlotStatus::Pending,
                            game_path: None,
                            score_a: None,
                            round: g,
                        });
                        id += 1;
                    }
                }
            }
        }
        TourneyFormat::Swiss => {
            append_swiss_round(&mut state, 0);
        }
    }
    state
}

/// Match points from finished slots (wins=1, draws=0.5).
pub fn match_scores(state: &TourneyState) -> BTreeMap<String, f64> {
    let mut scores = BTreeMap::new();
    for e in &state.entrants {
        scores.insert(e.id.clone(), 0.0);
    }
    for slot in &state.slots {
        if slot.status != SlotStatus::Done {
            continue;
        }
        let Some(sa) = slot.score_a else {
            continue;
        };
        *scores.entry(slot.model_a.clone()).or_insert(0.0) += sa;
        *scores.entry(slot.model_b.clone()).or_insert(0.0) += 1.0 - sa;
    }
    scores
}

fn prior_opponents(state: &TourneyState) -> HashSet<(String, String)> {
    let mut set = HashSet::new();
    for slot in &state.slots {
        if slot.status == SlotStatus::Aborted {
            continue;
        }
        let a = slot.model_a.clone();
        let b = slot.model_b.clone();
        if a <= b {
            set.insert((a, b));
        } else {
            set.insert((b, a));
        }
    }
    set
}

fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Games finished per entrant (each Done color-slot counts as one game).
pub fn games_played(state: &TourneyState) -> BTreeMap<String, usize> {
    let mut n = BTreeMap::new();
    for e in &state.entrants {
        n.insert(e.id.clone(), 0);
    }
    for slot in &state.slots {
        if slot.status != SlotStatus::Done {
            continue;
        }
        *n.entry(slot.model_a.clone()).or_insert(0) += 1;
        *n.entry(slot.model_b.clone()).or_insert(0) += 1;
    }
    n
}

/// Simple variety pairing: prefer low-game agents, similar scores, avoid rematches.
pub fn swiss_pairings(
    ids: &[String],
    games: &BTreeMap<String, usize>,
    scores: &BTreeMap<String, f64>,
    prior: &HashSet<(String, String)>,
) -> Vec<(String, String)> {
    let mut remaining: Vec<String> = ids.to_vec();
    remaining.sort_by(|a, b| {
        let ga = games.get(a).copied().unwrap_or(0);
        let gb = games.get(b).copied().unwrap_or(0);
        ga.cmp(&gb).then_with(|| a.cmp(b))
    });
    let mut out = Vec::new();
    while remaining.len() >= 2 {
        let a = remaining.remove(0);
        let score_a = scores.get(&a).copied().unwrap_or(0.0);
        // Prefer: not rematch, closest score, fewer games, then id.
        let mut best = 0usize;
        let mut best_key = (2u8, i64::MAX, usize::MAX, String::new());
        for (i, cand) in remaining.iter().enumerate() {
            let rematch = if prior.contains(&pair_key(&a, cand)) {
                1u8
            } else {
                0u8
            };
            let score_c = scores.get(cand).copied().unwrap_or(0.0);
            let score_diff = ((score_a - score_c).abs() * 1000.0).round() as i64;
            let g = games.get(cand).copied().unwrap_or(0);
            let key = (rematch, score_diff, g, cand.clone());
            if i == 0 || key < best_key {
                best = i;
                best_key = key;
            }
        }
        let b = remaining.remove(best);
        out.push((a, b));
    }
    out
}

fn append_swiss_round(state: &mut TourneyState, round: usize) {
    let ids: Vec<String> = state.entrants.iter().map(|e| e.id.clone()).collect();
    let games = games_played(state);
    let scores = match_scores(state);
    let prior = prior_opponents(state);
    let pairs = swiss_pairings(&ids, &games, &scores, &prior);
    let mut id = state.slots.iter().map(|s| s.id).max().map(|x| x + 1).unwrap_or(0);
    for (pi, (model_a, model_b)) in pairs.iter().enumerate() {
        for g in 0..state.games_per_pair.max(1) {
            let start_seed = state
                .seed_base
                .wrapping_add((round as u64).wrapping_mul(1_000_003))
                .wrapping_add((g as u64).wrapping_mul(97))
                .wrapping_add((pi as u64).wrapping_mul(17));
            for a_is_black in [true, false] {
                state.slots.push(TourneySlot {
                    id,
                    model_a: model_a.clone(),
                    model_b: model_b.clone(),
                    start_seed,
                    a_is_black,
                    status: SlotStatus::Pending,
                    game_path: None,
                    score_a: None,
                    round,
                });
                id += 1;
            }
        }
    }
    state.swiss_next_round = round + 1;
}

fn round_fully_settled(state: &TourneyState, round: usize) -> bool {
    let mut any = false;
    for s in &state.slots {
        if s.round != round {
            continue;
        }
        any = true;
        if s.status != SlotStatus::Done && s.status != SlotStatus::Aborted {
            return false;
        }
    }
    any
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
    let scores = match_scores(state);
    let mut rows: Vec<_> = state.elo.keys().cloned().collect();
    rows.sort_by(|a, b| {
        let sa = scores.get(a).copied().unwrap_or(0.0);
        let sb = scores.get(b).copied().unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let ea = state.elo.get(a).copied().unwrap_or(DEFAULT_ELO);
                let eb = state.elo.get(b).copied().unwrap_or(DEFAULT_ELO);
                eb.partial_cmp(&ea).unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let done = state
        .slots
        .iter()
        .filter(|s| s.status == SlotStatus::Done)
        .count();
    let total = state.slots.len();
    let mut out = String::new();
    out.push_str(&format!(
        "# Tournament {} standings ({:?})\n\nGames finished: {done}/{total}\n\n| Rank | Model | Score | Elo |\n|---:|---|---:|---:|\n",
        state.run_id, state.format
    ));
    for (i, id) in rows.iter().enumerate() {
        let elo = state.elo.get(id).copied().unwrap_or(DEFAULT_ELO);
        let sc = scores.get(id).copied().unwrap_or(0.0);
        out.push_str(&format!("| {} | {} | {:.1} | {:.1} |\n", i + 1, id, sc, elo));
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

    let state = if cfg.resume && state_path(cfg).exists() {
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
        if cfg.format == TourneyFormat::Swiss && cfg.swiss_rounds == 0 {
            return Err("swiss tournament needs --swiss-rounds >= 1".into());
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

    loop {
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

        if cfg_stop.load(Ordering::Relaxed) {
            break;
        }

        let mut st = state_mu.lock().unwrap();
        if st.format == TourneyFormat::Swiss
            && st.swiss_next_round < st.swiss_rounds.max(1)
            && st.swiss_next_round > 0
            && round_fully_settled(&st, st.swiss_next_round - 1)
        {
            let next = st.swiss_next_round;
            eprintln!("tournament: scheduling Swiss round {next}");
            append_swiss_round(&mut st, next);
            st.updated_at = now_secs();
            let _ = save_state(cfg, &st);
            continue;
        }
        break;
    }

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

    #[test]
    fn swiss_prefers_fresh_and_avoids_rematch() {
        let ids: Vec<String> = (0..4).map(|i| format!("p{i}")).collect();
        let mut games = BTreeMap::new();
        games.insert("p0".into(), 0);
        games.insert("p1".into(), 4);
        games.insert("p2".into(), 0);
        games.insert("p3".into(), 4);
        let scores = BTreeMap::new(); // all 0
        let mut prior = HashSet::new();
        prior.insert(pair_key("p0", "p2"));
        let pairs = swiss_pairings(&ids, &games, &scores, &prior);
        assert_eq!(pairs.len(), 2);
        // Lowest-game agents p0 and p2 are picked first; rematch avoided → p0↔p1 or p0↔p3.
        assert_ne!(pair_key(&pairs[0].0, &pairs[0].1), pair_key("p0", "p2"));
        let first = pair_key(&pairs[0].0, &pairs[0].1);
        assert!(first == pair_key("p0", "p1") || first == pair_key("p0", "p3"));
    }

    #[test]
    fn swiss_prefers_similar_score() {
        let ids: Vec<String> = (0..4).map(|i| format!("p{i}")).collect();
        let mut games = BTreeMap::new();
        for id in &ids {
            games.insert(id.clone(), 2);
        }
        let mut scores = BTreeMap::new();
        scores.insert("p0".into(), 2.0);
        scores.insert("p1".into(), 0.0);
        scores.insert("p2".into(), 2.0);
        scores.insert("p3".into(), 0.0);
        let prior = HashSet::new();
        let pairs = swiss_pairings(&ids, &games, &scores, &prior);
        assert_eq!(pairs.len(), 2);
        // p0 (2.0) should pair with p2 (2.0), not the 0.0 agents.
        assert_eq!(pair_key(&pairs[0].0, &pairs[0].1), pair_key("p0", "p2"));
    }

    #[test]
    fn swiss_schedule_grows_rounds() {
        let cfg = TourneyConfig {
            entrants: (0..4)
                .map(|i| TourneyEntrant {
                    id: format!("p{i}"),
                    model: format!("p{i}.json"),
                })
                .collect(),
            format: TourneyFormat::Swiss,
            swiss_rounds: 3,
            games_per_pair: 1,
            ..TourneyConfig::default()
        };
        let mut st = build_schedule(&cfg);
        // Round 0: 2 pairs × 2 colors = 4 slots
        assert_eq!(st.slots.len(), 4);
        assert_eq!(st.swiss_next_round, 1);
        for s in &mut st.slots {
            s.status = SlotStatus::Done;
            s.score_a = Some(1.0);
        }
        append_swiss_round(&mut st, 1);
        assert_eq!(st.slots.len(), 8);
        assert_eq!(st.swiss_next_round, 2);
        let r0: HashSet<_> = st
            .slots
            .iter()
            .filter(|s| s.round == 0 && s.a_is_black)
            .map(|s| pair_key(&s.model_a, &s.model_b))
            .collect();
        let r1: HashSet<_> = st
            .slots
            .iter()
            .filter(|s| s.round == 1 && s.a_is_black)
            .map(|s| pair_key(&s.model_a, &s.model_b))
            .collect();
        assert!(r0.is_disjoint(&r1));
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
