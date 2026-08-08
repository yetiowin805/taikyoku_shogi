//! Round-robin / continuous Swiss tournament with Glicko-1, checkpoint/resume, and cooperative stop.

use crate::game_history::GameResult;
use crate::training::paths::ensure_data_dirs;
use crate::training::pool::parse_starts_spec;
use crate::training::record::{AgentSpec, GameStart};
use crate::training::worker::{play_one_game, WorkerConfig};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_RATING: f64 = 1500.0;
pub const DEFAULT_RD: f64 = 350.0;
pub const RD_MIN: f64 = 30.0;
pub const PAIRING_WINDOW: f64 = 200.0;
pub const DEFAULT_GAMES_PER_PAIR: usize = 24;
/// Legacy constant (Swiss is continuous; kept for CLI/script compat).
pub const DEFAULT_SWISS_ROUNDS: usize = 0;

const GLICKO_Q: f64 = std::f64::consts::LN_10 / 400.0;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TourneyFormat {
    #[default]
    RoundRobin,
    /// Continuous Glicko-window pairing until cooperative stop.
    Swiss,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SlotStartMode {
    Opening,
    #[default]
    Light,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct GlickoRating {
    pub r: f64,
    pub rd: f64,
}

impl Default for GlickoRating {
    fn default() -> Self {
        Self {
            r: DEFAULT_RATING,
            rd: DEFAULT_RD,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
    /// Swiss round index (0-based). Round-robin uses games_per_pair index.
    #[serde(default)]
    pub round: usize,
    #[serde(default)]
    pub start_mode: SlotStartMode,
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
    /// Glicko-1 ratings (preferred).
    #[serde(default)]
    pub ratings: BTreeMap<String, GlickoRating>,
    /// Legacy Elo map; migrated into `ratings` on load when `ratings` empty.
    #[serde(default)]
    pub elo: BTreeMap<String, f64>,
    /// Unused (kept for old JSON).
    #[serde(default)]
    pub elo_k: f64,
    pub updated_at: u64,
    #[serde(default)]
    pub format: TourneyFormat,
    #[serde(default)]
    pub swiss_rounds: usize,
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

fn glicko_g(rd: f64) -> f64 {
    1.0 / (1.0 + 3.0 * GLICKO_Q * GLICKO_Q * rd * rd / (std::f64::consts::PI * std::f64::consts::PI))
        .sqrt()
}

/// One-game Glicko-1 update for both players (`score_a` in {0, 0.5, 1}).
pub fn glicko_update(a: GlickoRating, b: GlickoRating, score_a: f64) -> (GlickoRating, GlickoRating) {
    let na = glicko_update_one(a, b, score_a);
    let nb = glicko_update_one(b, a, 1.0 - score_a);
    (na, nb)
}

fn glicko_update_one(player: GlickoRating, opp: GlickoRating, score: f64) -> GlickoRating {
    let g = glicko_g(opp.rd);
    let e = 1.0 / (1.0 + 10f64.powf(-g * (player.r - opp.r) / 400.0));
    let d2 = 1.0 / (GLICKO_Q * GLICKO_Q * g * g * e * (1.0 - e));
    let rd_new = (1.0 / (1.0 / (player.rd * player.rd) + 1.0 / d2)).sqrt().max(RD_MIN);
    let r_new = player.r + GLICKO_Q / (1.0 / (rd_new * rd_new)) * g * (score - e);
    GlickoRating {
        r: r_new,
        rd: rd_new,
    }
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

fn ensure_ratings(state: &mut TourneyState) {
    if state.ratings.is_empty() && !state.elo.is_empty() {
        for (id, r) in &state.elo {
            state.ratings.insert(
                id.clone(),
                GlickoRating {
                    r: *r,
                    rd: DEFAULT_RD,
                },
            );
        }
    }
    for e in &state.entrants {
        state
            .ratings
            .entry(e.id.clone())
            .or_insert_with(GlickoRating::default);
    }
    // Mirror r into elo for legacy consumers.
    state.elo.clear();
    for (id, g) in &state.ratings {
        state.elo.insert(id.clone(), g.r);
    }
}

fn rating_of(state: &TourneyState, id: &str) -> GlickoRating {
    state
        .ratings
        .get(id)
        .copied()
        .unwrap_or_default()
}

fn build_schedule(cfg: &TourneyConfig) -> TourneyState {
    let mut ratings = BTreeMap::new();
    let mut elo = BTreeMap::new();
    for e in &cfg.entrants {
        ratings.insert(e.id.clone(), GlickoRating::default());
        elo.insert(e.id.clone(), DEFAULT_RATING);
    }
    let mut state = TourneyState {
        run_id: cfg.run_id.clone(),
        depth: cfg.depth,
        starts_spec: cfg.starts_spec.clone(),
        seed_base: cfg.seed_base,
        games_per_pair: cfg.games_per_pair,
        entrants: cfg.entrants.clone(),
        slots: Vec::new(),
        ratings,
        elo,
        elo_k: 0.0,
        updated_at: now_secs(),
        format: cfg.format,
        swiss_rounds: cfg.swiss_rounds,
        swiss_next_round: 0,
    };
    match cfg.format {
        TourneyFormat::RoundRobin => {
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
                            start_mode: SlotStartMode::Light,
                        });
                        id += 1;
                    }
                }
            }
        }
        TourneyFormat::Swiss => {
            // Continuous: slots are appended on demand.
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

fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn slot_counts(status: SlotStatus) -> bool {
    matches!(
        status,
        SlotStatus::Done | SlotStatus::Pending | SlotStatus::Running
    )
}

/// Games counted for pairing (Done + Pending + Running).
pub fn games_counted(state: &TourneyState) -> BTreeMap<String, usize> {
    let mut n = BTreeMap::new();
    for e in &state.entrants {
        n.insert(e.id.clone(), 0);
    }
    for slot in &state.slots {
        if !slot_counts(slot.status) {
            continue;
        }
        *n.entry(slot.model_a.clone()).or_insert(0) += 1;
        *n.entry(slot.model_b.clone()).or_insert(0) += 1;
    }
    n
}

/// Finished games only (for standings display).
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

fn busy_agents(state: &TourneyState) -> HashSet<String> {
    let mut busy = HashSet::new();
    for slot in &state.slots {
        if matches!(slot.status, SlotStatus::Pending | SlotStatus::Running) {
            busy.insert(slot.model_a.clone());
            busy.insert(slot.model_b.clone());
        }
    }
    busy
}

fn matchup_games(state: &TourneyState, a: &str, b: &str) -> usize {
    let mut n = 0usize;
    for slot in &state.slots {
        if !slot_counts(slot.status) {
            continue;
        }
        let key = pair_key(&slot.model_a, &slot.model_b);
        if key == pair_key(a, b) {
            n += 1;
        }
    }
    n
}

fn black_counts_overall(state: &TourneyState) -> BTreeMap<String, usize> {
    let mut n = BTreeMap::new();
    for e in &state.entrants {
        n.insert(e.id.clone(), 0);
    }
    for slot in &state.slots {
        if !slot_counts(slot.status) {
            continue;
        }
        let black = if slot.a_is_black {
            &slot.model_a
        } else {
            &slot.model_b
        };
        *n.entry(black.clone()).or_insert(0) += 1;
    }
    n
}

fn black_counts_matchup(state: &TourneyState, a: &str, b: &str) -> (usize, usize) {
    let mut ba = 0usize;
    let mut bb = 0usize;
    for slot in &state.slots {
        if !slot_counts(slot.status) {
            continue;
        }
        if pair_key(&slot.model_a, &slot.model_b) != pair_key(a, b) {
            continue;
        }
        let black = if slot.a_is_black {
            slot.model_a.as_str()
        } else {
            slot.model_b.as_str()
        };
        if black == a {
            ba += 1;
        } else if black == b {
            bb += 1;
        }
    }
    (ba, bb)
}

fn next_slot_id(state: &TourneyState) -> usize {
    state.slots.iter().map(|s| s.id).max().map(|x| x + 1).unwrap_or(0)
}

/// Pick continuous Swiss opponent pool for `a` among `candidates` (non-busy others).
pub fn opponent_pool(
    a: &str,
    candidates: &[String],
    ratings: &BTreeMap<String, GlickoRating>,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let ra = ratings.get(a).map(|g| g.r).unwrap_or(DEFAULT_RATING);
    let mut all_r: Vec<(String, f64)> = ratings
        .iter()
        .map(|(id, g)| (id.clone(), g.r))
        .collect();
    // Include candidates even if missing from ratings map.
    for c in candidates {
        if !all_r.iter().any(|(id, _)| id == c) {
            all_r.push((c.clone(), DEFAULT_RATING));
        }
    }
    let global_max = all_r
        .iter()
        .map(|(_, r)| *r)
        .fold(f64::NEG_INFINITY, f64::max);
    let global_min = all_r
        .iter()
        .map(|(_, r)| *r)
        .fold(f64::INFINITY, f64::min);
    let a_is_extreme = (ra - global_max).abs() < 1e-9 || (ra - global_min).abs() < 1e-9;

    let mut within: Vec<String> = candidates
        .iter()
        .filter(|c| {
            let rc = ratings.get(*c).map(|g| g.r).unwrap_or(DEFAULT_RATING);
            (rc - ra).abs() <= PAIRING_WINDOW
        })
        .cloned()
        .collect();

    if a_is_extreme {
        if within.len() > 2 {
            return within;
        }
        // Two closest by |r| (then id).
        let mut ranked: Vec<(f64, String)> = candidates
            .iter()
            .map(|c| {
                let rc = ratings.get(c).map(|g| g.r).unwrap_or(DEFAULT_RATING);
                ((rc - ra).abs(), c.clone())
            })
            .collect();
        ranked.sort_by(|x, y| {
            x.0.partial_cmp(&y.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| x.1.cmp(&y.1))
        });
        return ranked.into_iter().take(2).map(|(_, id)| id).collect();
    }

    let has_stronger = within.iter().any(|c| {
        ratings.get(c).map(|g| g.r).unwrap_or(DEFAULT_RATING) > ra + 1e-9
    });
    let has_weaker = within.iter().any(|c| {
        ratings.get(c).map(|g| g.r).unwrap_or(DEFAULT_RATING) < ra - 1e-9
    });
    if !has_stronger {
        if let Some(best) = candidates
            .iter()
            .filter(|c| ratings.get(*c).map(|g| g.r).unwrap_or(DEFAULT_RATING) > ra + 1e-9)
            .min_by(|x, y| {
                let rx = ratings.get(*x).map(|g| g.r).unwrap_or(DEFAULT_RATING);
                let ry = ratings.get(*y).map(|g| g.r).unwrap_or(DEFAULT_RATING);
                (rx - ra)
                    .abs()
                    .partial_cmp(&(ry - ra).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| x.cmp(y))
            })
        {
            if !within.contains(best) {
                within.push(best.clone());
            }
        }
    }
    if !has_weaker {
        if let Some(best) = candidates
            .iter()
            .filter(|c| ratings.get(*c).map(|g| g.r).unwrap_or(DEFAULT_RATING) < ra - 1e-9)
            .min_by(|x, y| {
                let rx = ratings.get(*x).map(|g| g.r).unwrap_or(DEFAULT_RATING);
                let ry = ratings.get(*y).map(|g| g.r).unwrap_or(DEFAULT_RATING);
                (rx - ra)
                    .abs()
                    .partial_cmp(&(ry - ra).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| x.cmp(y))
            })
        {
            if !within.contains(best) {
                within.push(best.clone());
            }
        }
    }
    within
}

fn assign_a_is_black(state: &TourneyState, a: &str, b: &str, slot_id: usize) -> bool {
    let (ba, bb) = black_counts_matchup(state, a, b);
    if ba < bb {
        return true; // a gets black
    }
    if bb < ba {
        return false;
    }
    // Matchup 50/50 → fewer blacks overall.
    let overall = black_counts_overall(state);
    let oa = overall.get(a).copied().unwrap_or(0);
    let ob = overall.get(b).copied().unwrap_or(0);
    if oa < ob {
        return true;
    }
    if ob < oa {
        return false;
    }
    // Tie → deterministic RNG from seed_base + slot id.
    let x = state.seed_base.wrapping_add(slot_id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (x >> 63) == 0
}

/// Schedule one continuous Swiss game. Returns false if no pair available.
pub fn schedule_one_swiss_game(state: &mut TourneyState) -> bool {
    ensure_ratings(state);
    let busy = busy_agents(state);
    let games = games_counted(state);
    let free: Vec<String> = state
        .entrants
        .iter()
        .map(|e| e.id.clone())
        .filter(|id| !busy.contains(id))
        .collect();
    if free.len() < 2 {
        return false;
    }

    // Pick A: least games, then highest r, then id.
    let a = free
        .iter()
        .min_by(|x, y| {
            let gx = games.get(*x).copied().unwrap_or(0);
            let gy = games.get(*y).copied().unwrap_or(0);
            gx.cmp(&gy).then_with(|| {
                let rx = rating_of(state, x).r;
                let ry = rating_of(state, y).r;
                ry.partial_cmp(&rx)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| x.cmp(y))
            })
        })
        .cloned()
        .expect("free >= 2");

    let candidates: Vec<String> = free.into_iter().filter(|id| id != &a).collect();
    let pool = opponent_pool(&a, &candidates, &state.ratings);
    if pool.is_empty() {
        return false;
    }

    let ra = rating_of(state, &a).r;
    let b = pool
        .iter()
        .min_by(|x, y| {
            let mx = matchup_games(state, &a, x);
            let my = matchup_games(state, &a, y);
            mx.cmp(&my).then_with(|| {
                let dx = (rating_of(state, x).r - ra).abs();
                let dy = (rating_of(state, y).r - ra).abs();
                dx.partial_cmp(&dy)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| x.cmp(y))
            })
        })
        .cloned()
        .expect("non-empty pool");

    let id = next_slot_id(state);
    let prior = matchup_games(state, &a, &b);
    let start_mode = if prior < 2 {
        SlotStartMode::Opening
    } else {
        SlotStartMode::Light
    };
    let a_is_black = assign_a_is_black(state, &a, &b, id);
    let start_seed = state
        .seed_base
        .wrapping_add((id as u64).wrapping_mul(1_000_003))
        .wrapping_add(97);

    state.slots.push(TourneySlot {
        id,
        model_a: a,
        model_b: b,
        start_seed,
        a_is_black,
        status: SlotStatus::Pending,
        game_path: None,
        score_a: None,
        round: 0,
        start_mode,
    });
    true
}

fn inflight_count(state: &TourneyState) -> usize {
    state
        .slots
        .iter()
        .filter(|s| matches!(s.status, SlotStatus::Pending | SlotStatus::Running))
        .count()
}

/// True when every scheduled RR slot is Done. Swiss continuous never completes on its own.
pub fn tournament_is_complete(state: &TourneyState) -> bool {
    match state.format {
        TourneyFormat::Swiss => false,
        TourneyFormat::RoundRobin => {
            !state.slots.is_empty()
                && state.slots.iter().all(|s| s.status == SlotStatus::Done)
        }
    }
}

/// Counts for notify / progress lines.
pub fn slot_status_counts(state: &TourneyState) -> (usize, usize, usize, usize) {
    let mut done = 0usize;
    let mut pending = 0usize;
    let mut running = 0usize;
    let mut aborted = 0usize;
    for s in &state.slots {
        match s.status {
            SlotStatus::Done => done += 1,
            SlotStatus::Pending => pending += 1,
            SlotStatus::Running => running += 1,
            SlotStatus::Aborted => aborted += 1,
        }
    }
    (done, pending, running, aborted)
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

fn ratings_path(cfg: &TourneyConfig) -> PathBuf {
    run_dir(cfg).join("ratings.json")
}

pub fn load_state(path: &Path) -> Result<TourneyState, String> {
    let s = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let mut state: TourneyState =
        serde_json::from_str(&s).map_err(|e| format!("parse {}: {e}", path.display()))?;
    ensure_ratings(&mut state);
    Ok(state)
}

pub fn save_state(cfg: &TourneyConfig, state: &TourneyState) -> Result<(), String> {
    let dir = run_dir(cfg);
    fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let mut state = state.clone();
    ensure_ratings(&mut state);
    let path = state_path(cfg);
    let json = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    let elo_json = serde_json::to_string_pretty(&state.elo).map_err(|e| e.to_string())?;
    fs::write(elo_path(cfg), elo_json).map_err(|e| e.to_string())?;
    let ratings_json = serde_json::to_string_pretty(&state.ratings).map_err(|e| e.to_string())?;
    fs::write(ratings_path(cfg), ratings_json).map_err(|e| e.to_string())?;
    fs::write(standings_path(cfg), format_standings(&state))
        .map_err(|e| format!("write standings: {e}"))?;
    Ok(())
}

pub fn format_standings(state: &TourneyState) -> String {
    let scores = match_scores(state);
    let games = games_played(state);
    let mut rows: Vec<_> = state.ratings.keys().cloned().collect();
    if rows.is_empty() {
        rows = state.elo.keys().cloned().collect();
    }
    rows.sort_by(|a, b| {
        let ra = rating_of(state, a).r;
        let rb = rating_of(state, b).r;
        rb.partial_cmp(&ra)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let sa = scores.get(a).copied().unwrap_or(0.0);
                let sb = scores.get(b).copied().unwrap_or(0.0);
                sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
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
        "# Tournament {} standings ({:?})\n\nGames finished: {done}/{total}\n\n| Rank | Model | Score | Games | Rating | RD |\n|---:|---|---:|---:|---:|---:|\n",
        state.run_id, state.format
    ));
    for (i, id) in rows.iter().enumerate() {
        let g = rating_of(state, id);
        let sc = scores.get(id).copied().unwrap_or(0.0);
        let n = games.get(id).copied().unwrap_or(0);
        out.push_str(&format!(
            "| {} | {} | {:.1} | {} | {:.1} | {:.1} |\n",
            i + 1,
            id,
            sc,
            n,
            g.r,
            g.rd
        ));
    }
    out.push('\n');
    out
}

pub fn standings_summary(state: &TourneyState) -> String {
    let mut rows: Vec<_> = state.ratings.iter().collect();
    rows.sort_by(|a, b| b.1.r.partial_cmp(&a.1.r).unwrap_or(std::cmp::Ordering::Equal));
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
    for (id, g) in rows {
        let delta = g.r - DEFAULT_RATING;
        lines.push(format!("{id}: {r:.1}±{rd:.1} (Δ{delta:+.1})", r = g.r, rd = g.rd));
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

fn resolve_start(
    format: TourneyFormat,
    start_mode: SlotStartMode,
    starts_spec: &str,
    start_seed: u64,
    light: &crate::training::pool::StartsSource,
) -> Result<GameStart, String> {
    match format {
        TourneyFormat::Swiss => match start_mode {
            SlotStartMode::Opening => Ok(GameStart::Opening),
            SlotStartMode::Light => light.start_for_seed(start_seed),
        },
        TourneyFormat::RoundRobin => {
            let starts = parse_starts_spec(starts_spec)?;
            starts.start_for_seed(start_seed)
        }
    }
}

/// Claim or schedule a Pending slot. Returns None when the worker should exit.
fn claim_or_schedule_slot(
    st: &mut TourneyState,
    cfg: &TourneyConfig,
) -> Option<usize> {
    if let Some(idx) = st.slots.iter().position(|s| s.status == SlotStatus::Pending) {
        st.slots[idx].status = SlotStatus::Running;
        st.updated_at = now_secs();
        return Some(st.slots[idx].id);
    }
    if st.format == TourneyFormat::Swiss
        && inflight_count(st) < cfg.jobs.max(1)
        && schedule_one_swiss_game(st)
    {
        // Newly scheduled slot is Pending; claim it.
        if let Some(idx) = st.slots.iter().position(|s| s.status == SlotStatus::Pending) {
            st.slots[idx].status = SlotStatus::Running;
            st.updated_at = now_secs();
            return Some(st.slots[idx].id);
        }
    }
    None
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
        ensure_ratings(&mut s);
        s
    } else {
        if cfg.entrants.len() < 2 {
            return Err("tournament needs at least 2 entrants".into());
        }
        build_schedule(cfg)
    };
    save_state(cfg, &state)?;

    let light_starts = parse_starts_spec("light")?;
    let jobs = cfg.jobs.max(1);
    let state_mu = Arc::new(Mutex::new(state));
    let cfg_stop = cfg.stop.clone();
    let games_dir = dir.clone();

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
                let light_starts = &light_starts;
                let games_dir = &games_dir;
                scope.spawn(move || loop {
                    poll_stop_file(cfg);
                    if cfg_stop.load(Ordering::Relaxed) {
                        return;
                    }

                    let slot_id = {
                        let mut st = state_mu.lock().unwrap();
                        match claim_or_schedule_slot(&mut st, cfg) {
                            Some(id) => {
                                let _ = save_state(cfg, &st);
                                id
                            }
                            None => return,
                        }
                    };

                    let (model_a, model_b, start_seed, a_is_black, depth, start_mode, format, starts_spec) = {
                        let st = state_mu.lock().unwrap();
                        let Some(slot) = st.slots.iter().find(|s| s.id == slot_id) else {
                            eprintln!("tournament: missing slot id {slot_id}");
                            return;
                        };
                        (
                            slot.model_a.clone(),
                            slot.model_b.clone(),
                            slot.start_seed,
                            slot.a_is_black,
                            st.depth,
                            slot.start_mode,
                            st.format,
                            st.starts_spec.clone(),
                        )
                    };

                    if cfg_stop.load(Ordering::Relaxed) {
                        let mut st = state_mu.lock().unwrap();
                        if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id) {
                            slot.status = SlotStatus::Aborted;
                        }
                        let _ = save_state(cfg, &st);
                        return;
                    }

                    let start = match resolve_start(
                        format,
                        start_mode,
                        &starts_spec,
                        start_seed,
                        light_starts,
                    ) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("tourney start error: {e}");
                            let mut st = state_mu.lock().unwrap();
                            if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id) {
                                slot.status = SlotStatus::Aborted;
                            }
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
                                drop(st);
                                let mut st = state_mu.lock().unwrap();
                                if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id)
                                {
                                    slot.status = SlotStatus::Aborted;
                                }
                                let _ = save_state(cfg, &st);
                                return;
                            }
                        };
                        let b = match agent_for(&st, &model_b, depth) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("{e}");
                                drop(st);
                                let mut st = state_mu.lock().unwrap();
                                if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id)
                                {
                                    slot.status = SlotStatus::Aborted;
                                }
                                let _ = save_state(cfg, &st);
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
                            ensure_ratings(&mut st);
                            let ra = rating_of(&st, &model_a);
                            let rb = rating_of(&st, &model_b);
                            let (na, nb) = glicko_update(ra, rb, score_a);
                            st.ratings.insert(model_a.clone(), na);
                            st.ratings.insert(model_b.clone(), nb);
                            st.elo.insert(model_a.clone(), na.r);
                            st.elo.insert(model_b.clone(), nb.r);
                            if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id) {
                                slot.status = SlotStatus::Done;
                                slot.score_a = Some(score_a);
                                slot.game_path = Some(path.display().to_string());
                            }
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
                            if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id) {
                                slot.status = SlotStatus::Aborted;
                            }
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

        // RR drained; Swiss must never exit until cooperative stop.
        let mut st = state_mu.lock().unwrap();
        if st.format == TourneyFormat::Swiss {
            if inflight_count(&st) < jobs && schedule_one_swiss_game(&mut st) {
                let _ = save_state(cfg, &st);
                continue;
            }
            // Wait for in-flight work, or idle-spin until stop / a free pair appears.
            drop(st);
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        break;
    }

    let mut state = state_mu.lock().unwrap().clone();
    for slot in &mut state.slots {
        if slot.status == SlotStatus::Running {
            slot.status = SlotStatus::Aborted;
        }
    }
    state.updated_at = now_secs();
    ensure_ratings(&mut state);
    save_state(cfg, &state)?;
    println!("{}", format_standings(&state));

    let stopped = cfg_stop.load(Ordering::Relaxed) || cfg.stop_file.exists();
    if state.format == TourneyFormat::Swiss {
        // Continuous Swiss only leaves the loop on cooperative stop.
        if stopped {
            return Ok(state);
        }
        return Err(format!(
            "tournament swiss exited without stop — resume with --resume --run-id {}",
            state.run_id
        ));
    }
    if !tournament_is_complete(&state) {
        let (done, pending, running, aborted) = slot_status_counts(&state);
        let why = if stopped { "stopped" } else { "incomplete" };
        return Err(format!(
            "tournament {why}: done={done} pending={pending} running={running} aborted={aborted} slots={} — resume with --resume --run-id {}",
            state.slots.len(),
            state.run_id
        ));
    }
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

    fn entrants(n: usize) -> Vec<TourneyEntrant> {
        (0..n)
            .map(|i| TourneyEntrant {
                id: format!("p{i}"),
                model: format!("p{i}.json"),
            })
            .collect()
    }

    #[test]
    fn glicko_favors_winner_and_shrinks_rd() {
        let a = GlickoRating::default();
        let b = GlickoRating::default();
        let (na, nb) = glicko_update(a, b, 1.0);
        assert!(na.r > DEFAULT_RATING);
        assert!(nb.r < DEFAULT_RATING);
        assert!(na.rd < DEFAULT_RD);
        assert!(nb.rd < DEFAULT_RD);
        assert!(na.rd >= RD_MIN);
    }

    #[test]
    fn glicko_high_rd_moves_more() {
        let newbie = GlickoRating {
            r: 1500.0,
            rd: 350.0,
        };
        let vet = GlickoRating {
            r: 1500.0,
            rd: 50.0,
        };
        let opp = GlickoRating {
            r: 1500.0,
            rd: 100.0,
        };
        let (n1, _) = glicko_update(newbie, opp, 1.0);
        let (v1, _) = glicko_update(vet, opp, 1.0);
        assert!((n1.r - 1500.0).abs() > (v1.r - 1500.0).abs());
    }

    #[test]
    fn schedule_size() {
        let cfg = TourneyConfig {
            entrants: entrants(3),
            games_per_pair: 2,
            ..TourneyConfig::default()
        };
        let st = build_schedule(&cfg);
        assert_eq!(st.slots.len(), 12);
    }

    #[test]
    fn schedule_interleaves_matchups_each_round() {
        let cfg = TourneyConfig {
            entrants: entrants(3),
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
        assert_eq!(first_round[0], ("p0", "p1"));
        assert_eq!(first_round[2], ("p0", "p2"));
        assert_eq!(first_round[4], ("p1", "p2"));
        assert_eq!(st.slots[6].model_a, "p0");
        assert_eq!(st.slots[6].model_b, "p1");
        assert_eq!(st.slots[0].start_seed, st.slots[1].start_seed);
        assert_ne!(st.slots[0].a_is_black, st.slots[1].a_is_black);
    }

    #[test]
    fn swiss_build_starts_empty() {
        let cfg = TourneyConfig {
            entrants: entrants(4),
            format: TourneyFormat::Swiss,
            ..TourneyConfig::default()
        };
        let st = build_schedule(&cfg);
        assert!(st.slots.is_empty());
        assert_eq!(st.ratings.len(), 4);
    }

    #[test]
    fn swiss_schedules_least_games_first() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(4),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        // Pretend p0 and p1 already played many games.
        st.ratings.insert(
            "p0".into(),
            GlickoRating {
                r: 1600.0,
                rd: 100.0,
            },
        );
        st.ratings.insert(
            "p1".into(),
            GlickoRating {
                r: 1550.0,
                rd: 100.0,
            },
        );
        for _ in 0..3 {
            st.slots.push(TourneySlot {
                id: next_slot_id(&st),
                model_a: "p0".into(),
                model_b: "p1".into(),
                start_seed: 0,
                a_is_black: true,
                status: SlotStatus::Done,
                game_path: None,
                score_a: Some(1.0),
                round: 0,
                start_mode: SlotStartMode::Opening,
            });
        }
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        // p2 and p3 have 0 games → one of them is A; opponent from pool.
        assert!(last.model_a == "p2" || last.model_a == "p3" || last.model_b == "p2" || last.model_b == "p3");
        let games = games_counted(&st);
        assert_eq!(games.get("p2").copied().unwrap_or(0), 1);
        assert_eq!(games.get("p3").copied().unwrap_or(0), 1);
    }

    #[test]
    fn swiss_first_two_opening_then_light() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(2),
            format: TourneyFormat::Swiss,
            seed_base: 7,
            ..TourneyConfig::default()
        });
        assert!(schedule_one_swiss_game(&mut st));
        assert_eq!(st.slots[0].start_mode, SlotStartMode::Opening);
        st.slots[0].status = SlotStatus::Done;
        st.slots[0].score_a = Some(1.0);
        assert!(schedule_one_swiss_game(&mut st));
        assert_eq!(st.slots[1].start_mode, SlotStartMode::Opening);
        st.slots[1].status = SlotStatus::Done;
        st.slots[1].score_a = Some(0.0);
        assert!(schedule_one_swiss_game(&mut st));
        assert_eq!(st.slots[2].start_mode, SlotStartMode::Light);
    }

    #[test]
    fn swiss_color_balances_matchup() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(2),
            format: TourneyFormat::Swiss,
            seed_base: 11,
            ..TourneyConfig::default()
        });
        assert!(schedule_one_swiss_game(&mut st));
        let first_black_is_a = st.slots[0].a_is_black;
        st.slots[0].status = SlotStatus::Done;
        st.slots[0].score_a = Some(0.5);
        assert!(schedule_one_swiss_game(&mut st));
        // Second game should flip colors in the matchup.
        assert_ne!(st.slots[1].a_is_black, first_black_is_a);
    }

    #[test]
    fn swiss_busy_agents_excluded() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(4),
            format: TourneyFormat::Swiss,
            ..TourneyConfig::default()
        });
        st.slots.push(TourneySlot {
            id: 0,
            model_a: "p0".into(),
            model_b: "p1".into(),
            start_seed: 0,
            a_is_black: true,
            status: SlotStatus::Running,
            game_path: None,
            score_a: None,
            round: 0,
            start_mode: SlotStartMode::Opening,
        });
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        assert!(last.model_a == "p2" || last.model_a == "p3");
        assert!(last.model_b == "p2" || last.model_b == "p3");
        assert_ne!(last.model_a, last.model_b);
    }

    #[test]
    fn opponent_pool_pads_missing_direction() {
        let mut ratings = BTreeMap::new();
        ratings.insert("a".into(), GlickoRating { r: 1500.0, rd: 100.0 });
        ratings.insert("strong".into(), GlickoRating { r: 1800.0, rd: 100.0 });
        ratings.insert("weak".into(), GlickoRating { r: 1200.0, rd: 100.0 });
        ratings.insert("near".into(), GlickoRating { r: 1510.0, rd: 100.0 });
        let cands = vec!["strong".into(), "weak".into(), "near".into()];
        let pool = opponent_pool("a", &cands, &ratings);
        // ±200 has near (stronger); no weaker inside window → pad closest weaker.
        assert!(pool.contains(&"near".to_string()));
        assert!(pool.contains(&"weak".to_string()));
        assert!(!pool.contains(&"strong".to_string()));

        // Only peers below A inside the window → pad closest stronger.
        let cands2 = vec!["strong".into(), "weak_near".into()];
        ratings.insert("weak_near".into(), GlickoRating { r: 1490.0, rd: 100.0 });
        let pool2 = opponent_pool("a", &cands2, &ratings);
        assert!(pool2.contains(&"weak_near".to_string()));
        assert!(pool2.contains(&"strong".to_string()));
    }

    #[test]
    fn opponent_pool_extreme_uses_two_closest_when_sparse() {
        let mut ratings = BTreeMap::new();
        ratings.insert("top".into(), GlickoRating { r: 2000.0, rd: 80.0 });
        ratings.insert("b".into(), GlickoRating { r: 1700.0, rd: 80.0 });
        ratings.insert("c".into(), GlickoRating { r: 1600.0, rd: 80.0 });
        ratings.insert("d".into(), GlickoRating { r: 1000.0, rd: 80.0 });
        let cands = vec!["b".into(), "c".into(), "d".into()];
        // Only b within 200 of top; extreme → two closest = b, c.
        let pool = opponent_pool("top", &cands, &ratings);
        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"b".to_string()));
        assert!(pool.contains(&"c".to_string()));
    }

    #[test]
    fn tournament_complete_rr_only() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(2),
            games_per_pair: 1,
            format: TourneyFormat::RoundRobin,
            ..TourneyConfig::default()
        });
        assert!(!tournament_is_complete(&st));
        for s in &mut st.slots {
            s.status = SlotStatus::Done;
            s.score_a = Some(0.5);
        }
        assert!(tournament_is_complete(&st));

        let swiss = build_schedule(&TourneyConfig {
            entrants: entrants(2),
            format: TourneyFormat::Swiss,
            ..TourneyConfig::default()
        });
        assert!(!tournament_is_complete(&swiss));
    }

    #[test]
    fn migrate_legacy_elo_into_ratings() {
        let mut st = TourneyState {
            run_id: "x".into(),
            depth: 2,
            starts_spec: "light".into(),
            seed_base: 1,
            games_per_pair: 1,
            entrants: entrants(2),
            slots: Vec::new(),
            ratings: BTreeMap::new(),
            elo: BTreeMap::from([("p0".into(), 1600.0), ("p1".into(), 1400.0)]),
            elo_k: 20.0,
            updated_at: 0,
            format: TourneyFormat::Swiss,
            swiss_rounds: 0,
            swiss_next_round: 0,
        };
        ensure_ratings(&mut st);
        assert!((st.ratings["p0"].r - 1600.0).abs() < 1e-9);
        assert!((st.ratings["p0"].rd - DEFAULT_RD).abs() < 1e-9);
    }
}

#[cfg(test)]
mod start_seed_tests {
    use super::*;
    use crate::training::pool::parse_starts_spec;
    use crate::training::record::GameStart;

    #[test]
    fn light_start_is_deterministic_for_seed() {
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
