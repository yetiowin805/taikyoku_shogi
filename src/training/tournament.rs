//! Round-robin / continuous Swiss tournament with Glicko-1, checkpoint/resume, and cooperative stop.

use crate::game_history::GameResult;
use crate::training::paths::ensure_data_dirs;
use crate::training::pool::parse_starts_spec;
use crate::training::record::{AgentSpec, GameStart};
use crate::training::worker::{play_one_game, WorkerConfig};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_RATING: f64 = 1500.0;
pub const DEFAULT_RD: f64 = 350.0;
pub const RD_MIN: f64 = 30.0;
/// Glicko-style passive RD growth per sit-out tick: rd' = min(DEFAULT_RD, sqrt(rd² + c²)).
pub const RD_PASSIVE_C: f64 = 15.0;
/// Finished games between passive RD ticks for agents who sat those games out.
pub const RD_PASSIVE_EVERY_N_GAMES: usize = 10;
/// Prefer scheduling agents below this counted-game floor before elite pairing.
pub const MINIMUM_GAMES: usize = 4;
/// After catch-up, always keep at least the top 2 by `r` in the pool
/// (plus anyone still UCI-elite). Avoids falling back to the whole field when
/// a runaway leader is the only agent with `r + RD + RD_leader > r_max`.
pub const ELITE_POOL_MIN: usize = 2;
/// After every agent has [`MINIMUM_GAMES`], 1 in N pairings is the current
/// leader vs a uniform-random opponent who fails the UCI elite bar
/// (`r + RD + RD_leader ≤ r_leader`).
pub const INFORMATIONAL_PAIR_DENOM: u64 = 10;
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
    /// Pinned historical binary; `None` = in-process current `ab`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
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
    /// Soft AB time budget (ms). On expiry, search returns the last completed ID depth.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_time_ms: Option<u64>,
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
    /// Finished games since the last passive RD tick.
    #[serde(default)]
    pub rd_tick_done_counter: usize,
    /// Agents who played in a finished game since the last passive RD tick.
    #[serde(default)]
    pub rd_tick_participants: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct TourneyConfig {
    pub run_id: String,
    pub outdir: PathBuf,
    pub entrants: Vec<TourneyEntrant>,
    pub starts_spec: String,
    pub depth: u32,
    /// Soft AB time budget (ms); `None` = fixed-depth search.
    pub max_time_ms: Option<u64>,
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
            max_time_ms: None,
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
        max_time_ms: cfg.max_time_ms,
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
        rd_tick_done_counter: 0,
        rd_tick_participants: BTreeSet::new(),
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

fn rating_of_map(ratings: &BTreeMap<String, GlickoRating>, id: &str) -> f64 {
    ratings.get(id).map(|g| g.r).unwrap_or(DEFAULT_RATING)
}

/// True when `agent` already has more than half its counted games vs `opponent`
/// and has more than 4 games. Soft rematch cap.
pub fn already_over_half_rematch(
    state: &TourneyState,
    games: &BTreeMap<String, usize>,
    agent: &str,
    opponent: &str,
) -> bool {
    let n = games.get(agent).copied().unwrap_or(0);
    if n <= 4 {
        return false;
    }
    let m = matchup_games(state, agent, opponent);
    m * 2 > n
}

/// Pick continuous Swiss opponent pool for `a` among `candidates`.
///
/// Starts with the 200-point window. If that has fewer than 2 names and at least
/// 2 candidates exist, expands by alternating the next unused higher/lower,
/// starting with the closer side.
pub fn opponent_pool(
    a: &str,
    candidates: &[String],
    ratings: &BTreeMap<String, GlickoRating>,
) -> Vec<String> {
    if candidates.is_empty() {
        return Vec::new();
    }
    // A singleton field must still be pairable, even outside the 200-point window.
    // Returning `within` here used to drop the only opponent and abort scheduling,
    // which made Swiss worker threads exit (`None => return`) and pin jobs at 1.
    if candidates.len() == 1 {
        return candidates.to_vec();
    }
    let ra = rating_of_map(ratings, a);
    let mut within: Vec<String> = candidates
        .iter()
        .filter(|c| (rating_of_map(ratings, c) - ra).abs() <= PAIRING_WINDOW)
        .cloned()
        .collect();
    if within.len() >= 2 {
        return within;
    }

    let used: HashSet<String> = within.iter().cloned().collect();
    let mut higher: Vec<(f64, String)> = candidates
        .iter()
        .filter(|c| !used.contains(*c) && rating_of_map(ratings, c) > ra + 1e-9)
        .map(|c| ((rating_of_map(ratings, c) - ra).abs(), c.clone()))
        .collect();
    let mut lower: Vec<(f64, String)> = candidates
        .iter()
        .filter(|c| !used.contains(*c) && rating_of_map(ratings, c) < ra - 1e-9)
        .map(|c| ((rating_of_map(ratings, c) - ra).abs(), c.clone()))
        .collect();
    let by_dist = |x: &(f64, String), y: &(f64, String)| {
        x.0.partial_cmp(&y.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| x.1.cmp(&y.1))
    };
    higher.sort_by(by_dist);
    lower.sort_by(by_dist);

    let mut hi = 0usize;
    let mut lo = 0usize;
    let mut take_higher = match (higher.first(), lower.first()) {
        (Some(h), Some(l)) => {
            h.0 < l.0 - 1e-12 || ((h.0 - l.0).abs() <= 1e-12 && h.1 <= l.1)
        }
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (None, None) => return within,
    };
    while within.len() < 2 && (hi < higher.len() || lo < lower.len()) {
        if take_higher && hi < higher.len() {
            within.push(higher[hi].1.clone());
            hi += 1;
        } else if !take_higher && lo < lower.len() {
            within.push(lower[lo].1.clone());
            lo += 1;
        } else if hi < higher.len() {
            within.push(higher[hi].1.clone());
            hi += 1;
        } else if lo < lower.len() {
            within.push(lower[lo].1.clone());
            lo += 1;
        }
        take_higher = !take_higher;
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

/// Highest-`r` rating in the field (ties: lowest id).
pub fn field_leader(state: &TourneyState) -> GlickoRating {
    state
        .entrants
        .iter()
        .map(|e| (rating_of(state, &e.id), &e.id))
        .max_by(|a, b| {
            a.0.r
                .partial_cmp(&b.0.r)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.1.cmp(a.1))
        })
        .map(|(g, _)| g)
        .unwrap_or_default()
}

pub fn field_max_rating(state: &TourneyState) -> f64 {
    field_leader(state).r
}

/// Upper-confidence elite vs the leader: `r + rd + rd_leader > r_leader`.
/// Uses the sum of both RDs, not `2 · rd` of the candidate alone.
pub fn elite_eligible(state: &TourneyState, id: &str) -> bool {
    let g = rating_of(state, id);
    let lead = field_leader(state);
    g.r + g.rd + lead.rd > lead.r
}

/// Highest-`r` entrant ids, ties broken by id. `n` is clamped to the field size.
pub fn top_rated_ids(state: &TourneyState, n: usize) -> BTreeSet<String> {
    let mut rows: Vec<(f64, String)> = state
        .entrants
        .iter()
        .map(|e| (rating_of(state, &e.id).r, e.id.clone()))
        .collect();
    rows.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    });
    rows.into_iter().take(n).map(|(_, id)| id).collect()
}

/// UCI-elite agents or the global top [`ELITE_POOL_MIN`] by `r`.
/// Falls back to all `names` only if that set has fewer than 2 (avoids deadlock).
pub fn swiss_pair_pool(state: &TourneyState, names: &[String]) -> Vec<String> {
    let top = top_rated_ids(state, ELITE_POOL_MIN);
    let pool: Vec<String> = names
        .iter()
        .filter(|id| elite_eligible(state, id) || top.contains(*id))
        .cloned()
        .collect();
    if pool.len() >= 2 {
        pool
    } else {
        names.to_vec()
    }
}

/// Deterministic 1-in-[`INFORMATIONAL_PAIR_DENOM`] roll for the next slot.
pub fn informational_pair_due(seed_base: u64, slot_id: usize) -> bool {
    pairing_u64(seed_base, slot_id, 0xA11C_E11E) % INFORMATIONAL_PAIR_DENOM == 0
}

fn pairing_u64(seed_base: u64, slot_id: usize, salt: u64) -> u64 {
    let mut x = seed_base
        .wrapping_add(slot_id as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt);
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn all_have_minimum_games(state: &TourneyState, games: &BTreeMap<String, usize>) -> bool {
    !state.entrants.is_empty()
        && state
            .entrants
            .iter()
            .all(|e| games.get(&e.id).copied().unwrap_or(0) >= MINIMUM_GAMES)
}

/// Agents who fail `r + RD + RD_leader > r_leader` (sorted for a stable pick).
pub fn out_of_range_opponents(state: &TourneyState, leader: &str, names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .filter(|id| id.as_str() != leader && !elite_eligible(state, id))
        .cloned()
        .collect();
    out.sort();
    out
}

fn pick_uniform(ids: &[String], seed_base: u64, slot_id: usize) -> String {
    let i = (pairing_u64(seed_base, slot_id, 0x0DD5) as usize) % ids.len();
    ids[i].clone()
}

/// Highest-`r` agent (tie: lowest id) vs a random UCI-ineligible opponent.
/// `None` if the roll misses or nobody is out of range.
fn try_informational_pair(state: &TourneyState, names: &[String]) -> Option<(String, String)> {
    let slot_id = next_slot_id(state);
    if !informational_pair_due(state.seed_base, slot_id) {
        return None;
    }
    let leader = top_rated_ids(state, 1).into_iter().next()?;
    if !names.iter().any(|id| id == &leader) {
        return None;
    }
    let outs = out_of_range_opponents(state, &leader, names);
    if outs.is_empty() {
        return None;
    }
    let b = pick_uniform(&outs, state.seed_base, slot_id);
    Some((leader, b))
}

/// Passive RD inflation for sit-outs (Glicko period decay shape).
pub fn apply_passive_rd_tick(rating: GlickoRating) -> GlickoRating {
    let rd = (rating.rd * rating.rd + RD_PASSIVE_C * RD_PASSIVE_C)
        .sqrt()
        .min(DEFAULT_RD)
        .max(RD_MIN);
    GlickoRating { r: rating.r, rd }
}

/// Record a finished game toward the every-N passive RD tick.
pub fn note_finished_game_for_rd_tick(state: &mut TourneyState, a: &str, b: &str) {
    state.rd_tick_participants.insert(a.to_string());
    state.rd_tick_participants.insert(b.to_string());
    state.rd_tick_done_counter += 1;
    if state.rd_tick_done_counter < RD_PASSIVE_EVERY_N_GAMES {
        return;
    }
    let played = std::mem::take(&mut state.rd_tick_participants);
    state.rd_tick_done_counter = 0;
    for e in state.entrants.clone() {
        if played.contains(&e.id) {
            continue;
        }
        let g = rating_of(state, &e.id);
        let ng = apply_passive_rd_tick(g);
        state.ratings.insert(e.id.clone(), ng);
        state.elo.insert(e.id, ng.r);
    }
}

fn pick_swiss_a(pool: &[String], state: &TourneyState, games: &BTreeMap<String, usize>) -> String {
    pool.iter()
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
        .expect("non-empty pool")
}

fn enqueue_swiss_slot(state: &mut TourneyState, a: String, b: String) {
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
}

pub fn schedule_one_swiss_game(state: &mut TourneyState) -> bool {
    ensure_ratings(state);
    let games = games_counted(state);
    // Engines are copies of weights, not humans: the same id may sit in many
    // in-flight games. Occupancy does not shrink the pairing pool; `jobs` is
    // the only concurrency cap.
    let names: Vec<String> = state.entrants.iter().map(|e| e.id.clone()).collect();
    if names.len() < 2 {
        return false;
    }

    let under: Vec<String> = names
        .iter()
        .filter(|id| games.get(*id).copied().unwrap_or(0) < MINIMUM_GAMES)
        .cloned()
        .collect();

    // Informational: only once the whole field has the game floor.
    if under.is_empty() && all_have_minimum_games(state, &games) {
        if let Some((a, b)) = try_informational_pair(state, &names) {
            enqueue_swiss_slot(state, a, b);
            return true;
        }
    }

    let elite_or_field = swiss_pair_pool(state, &names);

    // Catch-up: get every agent to MINIMUM_GAMES before elite gating.
    let (a, b_candidates) = if under.len() >= 2 {
        let a = pick_swiss_a(&under, state, &games);
        let rest: Vec<String> = under.into_iter().filter(|id| id != &a).collect();
        (a, rest)
    } else if under.len() == 1 {
        let a = under[0].clone();
        let rest: Vec<String> = elite_or_field.into_iter().filter(|id| id != &a).collect();
        let rest = if rest.is_empty() {
            names.iter().filter(|id| *id != &a).cloned().collect()
        } else {
            rest
        };
        (a, rest)
    } else {
        let a = pick_swiss_a(&elite_or_field, state, &games);
        let rest: Vec<String> = elite_or_field.into_iter().filter(|id| id != &a).collect();
        (a, rest)
    };

    let mut pool = opponent_pool(&a, &b_candidates, &state.ratings);
    if pool.is_empty() {
        let rest: Vec<String> = names.iter().filter(|id| *id != &a).cloned().collect();
        if rest != b_candidates {
            pool = opponent_pool(&a, &rest, &state.ratings);
        }
    }
    if pool.is_empty() {
        return false;
    }

    let preferred: Vec<String> = pool
        .iter()
        .filter(|b| {
            !already_over_half_rematch(state, &games, &a, b)
                && !already_over_half_rematch(state, &games, b, &a)
        })
        .cloned()
        .collect();
    let pick_from = if preferred.is_empty() { &pool } else { &preferred };

    let ra = rating_of(state, &a).r;
    let b = pick_from
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

    enqueue_swiss_slot(state, a, b);
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

fn agent_for(state: &TourneyState, id: &str, depth: u32) -> Result<AgentSpec, String> {
    let e = state
        .entrants
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| format!("unknown entrant {id}"))?;
    let mut a = AgentSpec::new("ab");
    a.depth = Some(depth);
    a.max_time_ms = state.max_time_ms;
    a.model = Some(e.model.clone());
    a.engine = e.engine.clone();
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

/// Claim a Pending slot, first topping the Swiss queue up to `jobs`.
/// Returns None when this worker has nothing to do right now (Swiss idle
/// threads must wait and retry — they must not exit, or jobs collapse to 1).
fn claim_or_schedule_slot(
    st: &mut TourneyState,
    cfg: &TourneyConfig,
) -> Option<usize> {
    if st.format == TourneyFormat::Swiss {
        let cap = cfg.jobs.max(1);
        while inflight_count(st) < cap && schedule_one_swiss_game(st) {}
    }
    if let Some(idx) = st.slots.iter().position(|s| s.status == SlotStatus::Pending) {
        st.slots[idx].status = SlotStatus::Running;
        st.updated_at = now_secs();
        return Some(st.slots[idx].id);
    }
    None
}

fn abort_claimed_slot(cfg: &TourneyConfig, st: &mut TourneyState, slot_id: usize) {
    if let Some(slot) = st.slots.iter_mut().find(|s| s.id == slot_id) {
        if slot.status != SlotStatus::Done {
            slot.status = SlotStatus::Aborted;
        }
    }
    st.updated_at = now_secs();
    let _ = save_state(cfg, st);
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
                    let slot_id = loop {
                        poll_stop_file(cfg);
                        if cfg_stop.load(Ordering::Relaxed) {
                            return;
                        }
                        let mut st = state_mu.lock().unwrap();
                        match claim_or_schedule_slot(&mut st, cfg) {
                            Some(id) => {
                                let _ = save_state(cfg, &st);
                                break id;
                            }
                            None if st.format == TourneyFormat::Swiss => {
                                drop(st);
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                            None => return,
                        }
                    };

                    let (model_a, model_b, start_seed, a_is_black, depth, start_mode, format, starts_spec) = {
                        let st = state_mu.lock().unwrap();
                        let Some(slot) = st.slots.iter().find(|s| s.id == slot_id) else {
                            eprintln!("tournament: missing slot id {slot_id}");
                            continue;
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
                        abort_claimed_slot(cfg, &mut st, slot_id);
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
                            abort_claimed_slot(cfg, &mut st, slot_id);
                            continue;
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
                                abort_claimed_slot(cfg, &mut st, slot_id);
                                continue;
                            }
                        };
                        let b = match agent_for(&st, &model_b, depth) {
                            Ok(b) => b,
                            Err(e) => {
                                eprintln!("{e}");
                                drop(st);
                                let mut st = state_mu.lock().unwrap();
                                abort_claimed_slot(cfg, &mut st, slot_id);
                                continue;
                            }
                        };
                        if a_is_black {
                            (a, b)
                        } else {
                            (b, a)
                        }
                    };

                    let play_seed = start_seed.wrapping_add(if a_is_black { 0 } else { 1 });
                    let play_cfg = WorkerConfig {
                        black,
                        white,
                        start,
                        seed: play_seed,
                        max_moves: cfg.max_moves,
                        verbose: cfg.verbose && jobs == 1,
                        stop: Some(Arc::clone(&cfg_stop)),
                    };
                    let play = catch_unwind(AssertUnwindSafe(|| play_one_game(&play_cfg)));
                    match play {
                        Ok(Ok(rec)) => {
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
                            note_finished_game_for_rd_tick(&mut st, &model_a, &model_b);
                            st.updated_at = now_secs();
                            let _ = save_state(cfg, &st);
                            if cfg.verbose {
                                println!(
                                    "slot {slot_id} done {} vs {} score_a={score_a:.1}",
                                    model_a, model_b
                                );
                            }
                        }
                        Ok(Err(e)) => {
                            let mut st = state_mu.lock().unwrap();
                            abort_claimed_slot(cfg, &mut st, slot_id);
                            if e.message != "stopped" {
                                eprintln!("slot {slot_id} failed: {}", e.message);
                            }
                            if cfg_stop.load(Ordering::Relaxed) {
                                return;
                            }
                        }
                        Err(_) => {
                            eprintln!("slot {slot_id} panicked; restarting worker immediately");
                            let mut st = state_mu.lock().unwrap();
                            abort_claimed_slot(cfg, &mut st, slot_id);
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
                engine: None,
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
    fn agent_for_sets_max_time_ms() {
        let st = build_schedule(&TourneyConfig {
            entrants: entrants(2),
            depth: 8,
            max_time_ms: Some(1000),
            ..TourneyConfig::default()
        });
        assert_eq!(st.max_time_ms, Some(1000));
        let a = agent_for(&st, "p0", st.depth).unwrap();
        assert_eq!(a.depth, Some(8));
        assert_eq!(a.max_time_ms, Some(1000));
        assert_eq!(a.model.as_deref(), Some("p0.json"));
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
    fn inflight_game_does_not_lock_agents() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(6),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        let rs = [1900.0, 1880.0, 1860.0, 1500.0, 1490.0, 1480.0];
        for i in 0..6 {
            st.ratings.insert(
                format!("p{i}"),
                GlickoRating {
                    r: rs[i],
                    rd: 40.0,
                },
            );
        }
        for i in 0..3 {
            give_min_games(&mut st, &format!("p{}", i * 2), &format!("p{}", i * 2 + 1), MINIMUM_GAMES);
        }
        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(elite_eligible(&st, "p0"));
        assert!(elite_eligible(&st, "p1"));
        assert!(elite_eligible(&st, "p2"));
        assert!(!elite_eligible(&st, "p3"));

        assert!(schedule_one_swiss_game(&mut st));
        st.slots.last_mut().unwrap().status = SlotStatus::Running;
        let first = st.slots.last().unwrap();
        let first_pair = [first.model_a.clone(), first.model_b.clone()];

        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(schedule_one_swiss_game(&mut st));
        let second = st.slots.last().unwrap();
        let reused = first_pair.iter().any(|id| id == &second.model_a || id == &second.model_b);
        assert!(
            reused,
            "expected a second in-flight game to reuse an engine copy, got {} vs {} after {} vs {}",
            second.model_a, second.model_b, first_pair[0], first_pair[1]
        );
        assert_ne!(second.model_a, second.model_b);
    }

    #[test]
    fn opponent_pool_expands_to_two_closer_side_first() {
        let mut ratings = BTreeMap::new();
        ratings.insert("a".into(), GlickoRating { r: 1500.0, rd: 100.0 });
        ratings.insert("strong".into(), GlickoRating { r: 1800.0, rd: 100.0 });
        ratings.insert("weak".into(), GlickoRating { r: 1200.0, rd: 100.0 });
        ratings.insert("near".into(), GlickoRating { r: 1510.0, rd: 100.0 });
        let cands = vec!["strong".into(), "weak".into(), "near".into()];
        let pool = opponent_pool("a", &cands, &ratings);
        // Window has only `near`; expand with closer unused side (tie 300 → id `strong`).
        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"near".to_string()));
        assert!(pool.contains(&"strong".to_string()));
        assert!(!pool.contains(&"weak".to_string()));

        let cands2 = vec!["strong".into(), "weak_near".into()];
        ratings.insert("weak_near".into(), GlickoRating { r: 1490.0, rd: 100.0 });
        let pool2 = opponent_pool("a", &cands2, &ratings);
        assert_eq!(pool2.len(), 2);
        assert!(pool2.contains(&"weak_near".to_string()));
        assert!(pool2.contains(&"strong".to_string()));
    }

    #[test]
    fn opponent_pool_singleton_outside_window_is_kept() {
        let mut ratings = BTreeMap::new();
        ratings.insert("a".into(), GlickoRating { r: 2000.0, rd: 80.0 });
        ratings.insert("far".into(), GlickoRating { r: 1700.0, rd: 80.0 });
        let cands = vec!["far".into()];
        let pool = opponent_pool("a", &cands, &ratings);
        assert_eq!(pool, vec!["far".to_string()]);
    }

    #[test]
    fn elite_blowout_still_schedules_then_fills_field() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(8),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        let rs = [2400.0, 2100.0, 1600.0, 1580.0, 1560.0, 1540.0, 1520.0, 1500.0];
        for i in 0..8 {
            st.ratings.insert(
                format!("p{i}"),
                GlickoRating {
                    r: rs[i],
                    rd: 40.0,
                },
            );
        }
        for i in 0..4 {
            give_min_games(&mut st, &format!("p{}", i * 2), &format!("p{}", i * 2 + 1), MINIMUM_GAMES);
        }
        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(elite_eligible(&st, "p0"));
        assert!(!elite_eligible(&st, "p1")); // 2100+40+40 ≤ 2400
        assert_eq!((rs[0] - rs[1]).abs() > PAIRING_WINDOW, true);

        assert!(schedule_one_swiss_game(&mut st));
        let first = st.slots.last().unwrap();
        let pair = [&first.model_a, &first.model_b];
        assert!(pair.contains(&&"p0".to_string()));
        assert!(pair.contains(&&"p1".to_string()));
        st.slots.last_mut().unwrap().status = SlotStatus::Running;

        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(schedule_one_swiss_game(&mut st));
        let second = st.slots.last().unwrap();
        let pair2 = [&second.model_a, &second.model_b];
        // Top 2 are not locked by the in-flight game; another copy can play.
        assert!(pair2.contains(&&"p0".to_string()));
        assert!(pair2.contains(&&"p1".to_string()));
    }

    #[test]
    fn opponent_pool_empty_window_still_has_two() {
        let mut ratings = BTreeMap::new();
        ratings.insert("a".into(), GlickoRating { r: 1500.0, rd: 80.0 });
        ratings.insert("hi".into(), GlickoRating { r: 1900.0, rd: 80.0 });
        ratings.insert("lo".into(), GlickoRating { r: 1100.0, rd: 80.0 });
        let cands = vec!["hi".into(), "lo".into()];
        let pool = opponent_pool("a", &cands, &ratings);
        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"hi".to_string()));
        assert!(pool.contains(&"lo".to_string()));
    }

    #[test]
    fn opponent_pool_leader_takes_next_two_lower() {
        let mut ratings = BTreeMap::new();
        ratings.insert("top".into(), GlickoRating { r: 2000.0, rd: 80.0 });
        ratings.insert("b".into(), GlickoRating { r: 1700.0, rd: 80.0 });
        ratings.insert("c".into(), GlickoRating { r: 1600.0, rd: 80.0 });
        ratings.insert("d".into(), GlickoRating { r: 1000.0, rd: 80.0 });
        let cands = vec!["b".into(), "c".into(), "d".into()];
        let pool = opponent_pool("top", &cands, &ratings);
        assert_eq!(pool.len(), 2);
        assert!(pool.contains(&"b".to_string()));
        assert!(pool.contains(&"c".to_string()));
    }

    #[test]
    fn tourney_entrant_engine_field_round_trips() {
        let e = TourneyEntrant {
            id: "LOGIC_H105".into(),
            model: "models/history/models/LOGIC_H105.json".into(),
            engine: Some("models/history/bin/LOGIC_H105".into()),
        };
        let text = serde_json::to_string(&e).unwrap();
        let back: TourneyEntrant = serde_json::from_str(&text).unwrap();
        assert_eq!(back.engine.as_deref(), Some("models/history/bin/LOGIC_H105"));
        let plain: TourneyEntrant = serde_json::from_str(r#"{"id":"SEED","model":"x.json"}"#).unwrap();
        assert!(plain.engine.is_none());
    }

    #[test]
    fn rematch_cap_skips_over_half_after_four_games() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(3),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        for id in ["p0", "p1", "p2"] {
            st.ratings.insert(
                id.into(),
                GlickoRating {
                    r: 1600.0,
                    rd: 40.0,
                },
            );
        }
        give_min_games(&mut st, "p0", "p1", 5);
        give_min_games(&mut st, "p2", "p1", MINIMUM_GAMES);
        assert!(already_over_half_rematch(
            &st,
            &games_counted(&st),
            "p0",
            "p1"
        ));
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(pair.contains(&&"p0".to_string()));
        assert!(pair.contains(&&"p2".to_string()));
        assert!(!pair.contains(&&"p1".to_string()));
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
            max_time_ms: None,
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
            rd_tick_done_counter: 0,
            rd_tick_participants: BTreeSet::new(),
        };
        ensure_ratings(&mut st);
        assert!((st.ratings["p0"].r - 1600.0).abs() < 1e-9);
        assert!((st.ratings["p0"].rd - DEFAULT_RD).abs() < 1e-9);
    }

    fn give_min_games(st: &mut TourneyState, a: &str, b: &str, n: usize) {
        for _ in 0..n {
            st.slots.push(TourneySlot {
                id: next_slot_id(st),
                model_a: a.into(),
                model_b: b.into(),
                start_seed: 0,
                a_is_black: true,
                status: SlotStatus::Done,
                game_path: None,
                score_a: Some(1.0),
                round: 0,
                start_mode: SlotStartMode::Opening,
            });
        }
    }

    fn seed_for_info_pair(slot_id: usize, want: bool) -> u64 {
        (0u64..10_000)
            .find(|&s| informational_pair_due(s, slot_id) == want)
            .expect("found a seed for the informational roll")
    }

    fn runaway_field(st: &mut TourneyState) {
        let rs = [
            2400.0, 2200.0, 2150.0, 2100.0, 2050.0, 2000.0, 1950.0, 1900.0, 1700.0, 1600.0,
        ];
        for i in 0..10 {
            st.ratings.insert(
                format!("p{i}"),
                GlickoRating {
                    r: rs[i],
                    rd: 40.0,
                },
            );
        }
        for i in 0..5 {
            give_min_games(st, &format!("p{}", i * 2), &format!("p{}", i * 2 + 1), MINIMUM_GAMES);
        }
    }

    #[test]
    fn elite_pool_excludes_low_uci_agents() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(10),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        // p0 leader; p2/p9 are 9th/10th by r and fail UCI; p3 is UCI via large RD.
        let rs = [
            1900.0, 1850.0, 1500.0, 1700.0, 1650.0, 1640.0, 1630.0, 1620.0, 1610.0, 1400.0,
        ];
        let rds = [40.0, 40.0, 50.0, 220.0, 40.0, 40.0, 40.0, 40.0, 40.0, 40.0];
        for i in 0..10 {
            st.ratings.insert(
                format!("p{i}"),
                GlickoRating {
                    r: rs[i],
                    rd: rds[i],
                },
            );
        }
        for i in 0..5 {
            give_min_games(&mut st, &format!("p{}", i * 2), &format!("p{}", i * 2 + 1), MINIMUM_GAMES);
        }
        give_min_games(&mut st, "p0", "p1", 2);
        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(elite_eligible(&st, "p0"));
        assert!(elite_eligible(&st, "p1")); // 1850+40+40 > 1900
        assert!(!elite_eligible(&st, "p2")); // 1500+50+40 ≤ 1900
        assert!(elite_eligible(&st, "p3")); // 1700+220+40 > 1900
        let top = top_rated_ids(&st, ELITE_POOL_MIN);
        assert!(top.contains("p0") && top.contains("p1"));
        assert!(!top.contains("p2") && !top.contains("p8"));
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(!pair.contains(&&"p2".to_string()));
        assert!(!pair.contains(&&"p9".to_string()));
    }

    #[test]
    fn top2_floor_used_when_uci_elite_is_singleton() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(10),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        // Runaway leader: only p0 is UCI-elite. Pool must still be the top 2 by r.
        let rs = [
            2400.0, 2200.0, 2150.0, 2100.0, 2050.0, 2000.0, 1950.0, 1900.0, 1700.0, 1600.0,
        ];
        for i in 0..10 {
            st.ratings.insert(
                format!("p{i}"),
                GlickoRating {
                    r: rs[i],
                    rd: 40.0,
                },
            );
        }
        for i in 0..5 {
            give_min_games(&mut st, &format!("p{}", i * 2), &format!("p{}", i * 2 + 1), MINIMUM_GAMES);
        }
        st.seed_base = seed_for_info_pair(next_slot_id(&st), false);
        assert!(elite_eligible(&st, "p0"));
        assert!(!elite_eligible(&st, "p1")); // 2200+40+40 ≤ 2400
        let free: Vec<String> = (0..10).map(|i| format!("p{i}")).collect();
        let pool = swiss_pair_pool(&st, &free);
        assert_eq!(pool.len(), ELITE_POOL_MIN);
        assert!(pool.contains(&"p0".to_string()) && pool.contains(&"p1".to_string()));
        assert!(!pool.contains(&"p2".to_string()) && !pool.contains(&"p9".to_string()));
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(!pair.contains(&&"p2".to_string()));
        assert!(!pair.contains(&&"p9".to_string()));
    }

    #[test]
    fn minimum_games_catchup_overrides_elite() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(4),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        st.ratings.insert(
            "p0".into(),
            GlickoRating {
                r: 1900.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p1".into(),
            GlickoRating {
                r: 1850.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p2".into(),
            GlickoRating {
                r: 1500.0,
                rd: 50.0,
            },
        );
        st.ratings.insert(
            "p3".into(),
            GlickoRating {
                r: 1490.0,
                rd: 50.0,
            },
        );
        for _ in 0..MINIMUM_GAMES {
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
        // p2/p3 have 0 games and are outside elite; catch-up must still pair them.
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(pair.contains(&&"p2".to_string()));
        assert!(pair.contains(&&"p3".to_string()));
    }

    #[test]
    fn passive_rd_tick_inflates_sitouts_every_n() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(3),
            format: TourneyFormat::Swiss,
            ..TourneyConfig::default()
        });
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
        st.ratings.insert(
            "p2".into(),
            GlickoRating {
                r: 1500.0,
                rd: 100.0,
            },
        );
        for _ in 0..RD_PASSIVE_EVERY_N_GAMES {
            note_finished_game_for_rd_tick(&mut st, "p0", "p1");
        }
        assert_eq!(st.rd_tick_done_counter, 0);
        assert!(st.rd_tick_participants.is_empty());
        let expect = (100.0f64 * 100.0 + RD_PASSIVE_C * RD_PASSIVE_C).sqrt();
        assert!((st.ratings["p2"].rd - expect).abs() < 1e-9);
        assert!((st.ratings["p0"].rd - 100.0).abs() < 1e-9);
        assert!((st.ratings["p1"].rd - 100.0).abs() < 1e-9);
    }

    #[test]
    fn informational_pair_hits_leader_vs_uci_ineligible() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(10),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        runaway_field(&mut st);
        st.seed_base = seed_for_info_pair(next_slot_id(&st), true);
        assert!(!elite_eligible(&st, "p2"));
        assert!(!elite_eligible(&st, "p9"));
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(pair.contains(&&"p0".to_string()));
        let opp = if last.model_a == "p0" {
            last.model_b.as_str()
        } else {
            last.model_a.as_str()
        };
        assert_ne!(opp, "p0");
        assert!(!elite_eligible(&st, opp));
    }

    #[test]
    fn informational_pair_waits_for_minimum_games() {
        let mut st = build_schedule(&TourneyConfig {
            entrants: entrants(4),
            format: TourneyFormat::Swiss,
            seed_base: 1,
            ..TourneyConfig::default()
        });
        st.ratings.insert(
            "p0".into(),
            GlickoRating {
                r: 2400.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p1".into(),
            GlickoRating {
                r: 2200.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p2".into(),
            GlickoRating {
                r: 1500.0,
                rd: 40.0,
            },
        );
        st.ratings.insert(
            "p3".into(),
            GlickoRating {
                r: 1400.0,
                rd: 40.0,
            },
        );
        give_min_games(&mut st, "p0", "p1", MINIMUM_GAMES);
        st.seed_base = seed_for_info_pair(next_slot_id(&st), true);
        assert!(schedule_one_swiss_game(&mut st));
        let last = st.slots.last().unwrap();
        let pair = [&last.model_a, &last.model_b];
        assert!(pair.contains(&&"p2".to_string()));
        assert!(pair.contains(&&"p3".to_string()));
        assert!(!pair.contains(&&"p0".to_string()));
    }

    #[test]
    fn informational_pair_rate_is_one_in_denom() {
        let hits = (0usize..10_000)
            .filter(|&id| informational_pair_due(1, id))
            .count();
        assert!((900..=1100).contains(&hits), "hits={hits}");
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
