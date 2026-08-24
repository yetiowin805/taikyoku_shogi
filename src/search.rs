//! Alpha-beta search over GameState with make/unmake, compact traces for the GUI.
//!
//! Pipeline, PathAware q, and what each prune can miss: `src/README.md`.

use crate::eval::{
    bind_search_weights, evaluate_with_ply, is_big_piece, is_range_two_mover, material_piece_value,
    promotes_into_big_piece, seed_loud_capture_floor, EvalWeights,
};
use crate::game_state::{GameState, LegalMoveGen, Move};
use crate::move_simulation::BoardLike;
use crate::movement::{BlockingMode, MovementCapability, MovementConfig, MovementGenerator};
use crate::path_utils;
use crate::piece::{Color, Piece};
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Max root moves kept in the GUI tree (best + alternatives).
pub const MAX_TREE_ROOT_CHILDREN: usize = 12;
/// Max children kept under a non-root tree node.
pub const MAX_TREE_BRANCH: usize = 8;

/// Cap unique-q hash tracking (memory); report saturated if hit.
const Q_UNIQUE_CAP: usize = 65_536;

/// Experimental / production quiescence capture filters.
///
/// Measurement (`qprune_mode_matrix`, post-GG leaf q=6):
/// - **TopN**: ~4.4× fewer q-nodes, score ~5 vs baseline 7
/// - **NetGain**: little leaf effect; fewer AB nodes on opening
/// - **RecaptureOnly**: ~200× cut but score 90 vs 7 (over-prune) — not default
/// - **StaleHang**: no meaningful cut on the GG blowup
/// - **PathAware**: ~37× fewer q-nodes vs baseline on post-GG leaf (58 vs 2158),
///   score 127 vs 126; ~3× fewer than A+B alone — shipped default.
///   Tuned further with first-ply TopN=2, deep TopN=3, PathClear/MultiLeg
///   deep budget ([`QUIESCE_PATHCLEAR_DEEP_BUDGET`]), and deep SimpleTakes
///   restricted to recaptures onto the previous landing square.
///
/// Default: [`QPruneMode::PathAware`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QPruneMode {
    /// Existing: free-capture delta only (`stand_pat + enemy > α`).
    Baseline,
    /// A: delta uses net path material `enemy - own`.
    NetGainDelta,
    /// B: after MVV order, search at most [`QUIESCE_TOP_N`] captures.
    TopN,
    /// C: after the first q-ply, only recaptures onto the previous landing square.
    RecaptureOnly,
    /// D: drop captures where landing looks attacked and net < frac*mover (stale).
    StaleHang,
    /// A+B — previous default after first measurement pass.
    NetGainAndTopN,
    /// B+C (kept for harness; recapture over-pruned alone).
    TopNAndRecapture,
    /// Path-aware quietness: A+B + SimpleTake hang + deep loudness/fanout taper.
    #[default]
    PathAware,
}

/// Max captures expanded per q-node at the first quiescence ply.
pub const QUIESCE_TOP_N: usize = 8;
/// PathAware first ply: keep fanout small — GG/BG PathClears dominate here.
pub const QUIESCE_TOP_N_PATH_AWARE_ROOT: usize = 2;
/// Max captures at deeper q-plies under [`QPruneMode::PathAware`].
pub const QUIESCE_TOP_N_DEEP: usize = 3;
/// At deep q-plies, expand at most this many PathClear/MultiLeg captures.
pub const QUIESCE_PATHCLEAR_DEEP_BUDGET: usize = 1;
/// Hang / demote when net material gain is below this fraction of mover value.
pub const HANG_NET_FRAC: f32 = 0.8;
/// AB skips captures that hang a mover at or above this piece value.
pub const HIGH_VALUE_HANGER: f32 = 400.0;
/// Scores at or above this are mate / last-royal wins. Order those first and
/// stop the root loop once one is found.
const MATE_SCORE_BAND: i32 = 900_000;
/// Move-order key so last-royal captures sort above every MVV capture.
const LAST_ROYAL_ORDER: i32 = i32::MAX / 2;

/// Deeper-ply loudness floor (same formula as root worthwhile threshold).
pub fn min_quiescence_deep_enemy() -> f32 {
    seed_loud_capture_floor()
}

/// PathAware quiescence: expand PathClear/MultiLeg only as a destination
/// recapture onto the previous landing (`mv.to == prev_to`).
///
/// Q finishes the contested square; capturing-range corridor wipes belong to AB.
fn pathclear_allowed_in_pathaware_q(is_dest_recapture: bool) -> bool {
    is_dest_recapture
}

/// Enemy on `prev_to` is a major piece that just landed. Dest recaptures must
/// run before a stand-pat β cutoff (and stay outside TopN / live delta).
fn prev_to_is_major_enemy(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Option<Position>,
) -> bool {
    let Some(sq) = prev_to else {
        return false;
    };
    let Some(piece) = state.get_board().get_piece(sq) else {
        return false;
    };
    if piece.color == state.get_current_turn() {
        return false;
    }
    is_big_piece(piece.piece_type)
        || material_piece_value(&piece, weights) >= min_quiescence_enemy_material()
}

/// PathClear empty-beyond landings that share `(from, direction, occupied-between)`.
/// Landing on a victim is `None` (always full-search). Empty rays are `None`.
fn capturing_wipe_group_key(state: &GameState, mv: &Move) -> Option<u64> {
    if mv.is_two_step() || mv.is_free_eagle() {
        return None;
    }
    let board = state.get_board();
    let piece = board.get_piece(mv.from)?;
    if !piece_has_capturing_range(&piece) {
        return None;
    }
    if board.get_piece(mv.to).is_some() {
        return None;
    }
    let file_diff = mv.to.file as i8 - mv.from.file as i8;
    let rank_diff = mv.to.rank as i8 - mv.from.rank as i8;
    if file_diff == 0 && rank_diff == 0 {
        return None;
    }
    let file_step = if file_diff == 0 {
        0
    } else if file_diff > 0 {
        1
    } else {
        -1
    };
    let rank_step = if rank_diff == 0 {
        0
    } else if rank_diff > 0 {
        1
    } else {
        -1
    };
    if file_step != 0 && rank_step != 0 && file_diff.abs() != rank_diff.abs() {
        return None;
    }
    if file_step != 0 && rank_step == 0 && rank_diff != 0 {
        return None;
    }
    if file_step == 0 && rank_step != 0 && file_diff != 0 {
        return None;
    }

    let mut occ = 0u64;
    let mut n = 0u32;
    for pos in path_utils::get_path_positions(mv.from, mv.to) {
        if pos == mv.to {
            continue;
        }
        if board.get_piece(pos).is_some() {
            n += 1;
            occ ^= 0x9E3779B97F4A7C15u64.wrapping_mul(1 + pos.to_index() as u64);
            occ = occ.rotate_left(7);
        }
    }
    if n == 0 {
        return None;
    }
    let dir = (file_step as u8 as u64) | ((rank_step as u8 as u64) << 3);
    Some(
        occ ^ (mv.from.to_index() as u64).wrapping_mul(0x100000001B3)
            ^ dir.wrapping_mul(0xD1B54A32D192ED03)
            ^ (n as u64).wrapping_mul(0xC2B2AE3D27D4EB4F),
    )
}

enum SiblingAction {
    Full,
    Reduce { r: u32, rel_expected: Option<i32> },
    Static { expected: i32 },
}

fn sibling_action(
    mode: u8,
    key: Option<u64>,
    reps: &HashMap<u64, (i32, i32)>,
    e_i: i32,
    landing_attacked: bool,
    child_depth: u32,
    is_first_move: bool,
) -> SiblingAction {
    if mode == 0 || is_first_move || landing_attacked || child_depth == 0 {
        return SiblingAction::Full;
    }
    let Some(k) = key else {
        return SiblingAction::Full;
    };
    let Some(&(s_rep, e_rep)) = reps.get(&k) else {
        return SiblingAction::Full;
    };
    let expected = s_rep.saturating_add(e_i.saturating_sub(e_rep));
    match mode {
        1 => SiblingAction::Reduce {
            r: 1.min(child_depth),
            rel_expected: None,
        },
        2 => SiblingAction::Reduce {
            r: 2.min(child_depth),
            rel_expected: None,
        },
        3 => SiblingAction::Reduce {
            r: 1.min(child_depth),
            rel_expected: Some(expected),
        },
        4 => SiblingAction::Static { expected },
        _ => SiblingAction::Full,
    }
}

/// How a capture removes material (drives PathAware hang / taper rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureKind {
    /// Single-square enemy take (no path clears / multi-leg).
    SimpleTake,
    /// Capturing-range path edit (may clear many squares).
    PathClear,
    /// Two-step or FreeEagle multi-leg.
    MultiLeg,
}

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub depth: u32,
    pub max_time_ms: Option<u64>,
    /// When true, build multipv root lines + reply trees for the GUI.
    /// Does not change which move is selected as best.
    pub collect_trace: bool,
    /// Capture-only quiescence plies at main-search leaves (0 = off).
    pub quiescence_depth: u32,
    /// Quiescence capture prune policy.
    pub q_prune_mode: QPruneMode,
    /// TT cluster width. `1` is the production single-slot table.
    pub tt_clusters: u8,
    /// Mix the contested square (`prev_to`) into the q TT key.
    pub q_hash_prev_to: bool,
    /// After a PathClear/MultiLeg in q, children may not PathClear (D1).
    pub q_no_pathclear_reply: bool,
    /// Never expand PathClear/MultiLeg in q (SimpleTake dest recaptures remain).
    pub q_no_pathclear: bool,
    /// Loud-promo exemption applies only to SimpleTakes (not PathClear/MultiLeg).
    pub q_loud_promo_simple_only: bool,
    /// Count unique q hashes in release (capped). Off in production.
    pub track_q_unique: bool,
    /// Same-wipe PathClear sibling search: 0 off, 1 LMR R=1, 2 LMR R=2,
    /// 3 rel-window R=1, 4 static `S_rep+δ` (no probe).
    pub sibling_mode: u8,
    /// After an AB PathClear/MultiLeg, q may not expand PathClear (incl. loud RO+).
    pub q_no_pathclear_after_wipe: bool,
    /// Dest MultiLeg takes of hanging range two-movers open q (default on).
    pub hang_q_dest_multileg: bool,
    /// Dest PathClear takes of hanging range two-movers open q (default on).
    pub hang_q_dest_pathclear: bool,
    /// Open q after a capture by a large mover (`is_big_piece`).
    pub q_open_large_mover: bool,
    /// Open q after any enemy capture (not only loud / hang / royal).
    pub q_open_any_capture: bool,
    /// Keep dest recaptures onto `prev_to` (plus royals / loud promos).
    pub q_recapture_only: bool,
    /// Open q for dest recapture of a large landing; keep only large-victim takes.
    pub q_own_large_only: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            depth: 2,
            max_time_ms: None,
            collect_trace: false,
            quiescence_depth: 2,
            // PathAware: net-gain + top-N with SimpleTake hang + deep taper.
            q_prune_mode: QPruneMode::PathAware,
            tt_clusters: 1,
            q_hash_prev_to: false,
            q_no_pathclear_reply: false,
            q_no_pathclear: false,
            q_loud_promo_simple_only: false,
            track_q_unique: false,
            sibling_mode: 0,
            q_no_pathclear_after_wipe: false,
            hang_q_dest_multileg: true,
            hang_q_dest_pathclear: true,
            q_open_large_mover: false,
            q_open_any_capture: false,
            q_recapture_only: false,
            q_own_large_only: false,
        }
    }
}

/// One root candidate after search (STM perspective).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootMoveInfo {
    pub label: String,
    pub score: i32,
    pub best: bool,
}

/// Compact node for GUI tree visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTreeNode {
    pub label: String,
    pub score: Option<i32>,
    pub static_eval: Option<i32>,
    pub best: bool,
    pub cutoff: bool,
    pub children: Vec<SearchTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchInfo {
    pub agent: String,
    /// Side that performed this search (Black / White).
    pub side: String,
    pub depth: u32,
    pub nodes: u64,
    /// Static eval before the move (side-to-move perspective).
    pub static_eval: i32,
    /// Search score of the chosen move (STM perspective).
    pub score: i32,
    pub best_move: Option<String>,
    /// Root candidates, best first, capped for display.
    pub root_moves: Vec<RootMoveInfo>,
    pub tree: SearchTreeNode,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub best_move: Option<Move>,
    pub score: i32,
    pub nodes: u64,
    pub static_eval: i32,
    pub root_lines: Vec<(Move, i32)>,
    pub tree: SearchTreeNode,
    /// Quiescence nodes visited.
    pub q_nodes: u64,
    /// Capture candidates after delta prune (summed over q-nodes).
    pub q_caps_generated: u64,
    /// Captures actually recursed into.
    pub q_caps_searched: u64,
    pub q_tt_hits: u64,
    pub q_tt_probes: u64,
    pub q_kind_path: u64,
    pub q_kind_simple: u64,
    pub q_kind_multi: u64,
    pub q_unique: u64,
    pub q_unique_saturated: bool,
    pub root_moves_scored: u64,
    pub aborted: bool,
}

struct SearchContext {
    deadline: Option<Instant>,
    nodes: u64,
    abort: bool,
    /// Ply counter for eval noise (does not rely on move_history during search).
    ply: usize,
    quiescence_depth: u32,
    /// Entry q-ply budget for the current quiescence call (PathAware deep_ply).
    quiesce_entry_depth: u32,
    /// Wall-clock start of this `search()` call.
    search_started: Instant,
    last_progress_log: Instant,
    /// Main search depth (for progress logs).
    search_depth: u32,
    /// Root move currently being searched (1-based index / total).
    root_index: usize,
    root_total: usize,
    root_label: String,
    best_score: i32,
    /// Short phase tag for logs: "root", "search", "quiesce", "trace".
    phase: &'static str,
    tt: TranspositionTable,
    /// Dedicated TT for quiescence (depths are q-plies, not AB plies).
    q_tt: TranspositionTable,
    /// Two killer quiets per ply (cutoff memory for capture-setups).
    killers: Vec<[Option<MoveKey>; 2]>,
    /// Quiet history: (from_index << 16) | to_index → score.
    history: HashMap<u32, i32>,
    /// Disallow consecutive null moves.
    allow_null: bool,
    /// Enemy material taken by the AB move that entered this node (0 = quiet).
    /// Loud parents (≥ floor) open capture q; quiet parents may still open q for
    /// loud promos or lesser-valued SimpleTakes of hanging large pieces.
    last_ab_capture_enemy: f32,
    /// Landing of the last AB move (search skips move_history). Seeds q prev_to.
    last_ab_to: Option<Position>,
    /// Last AB move was PathClear/MultiLeg (corridor wipe).
    last_ab_wipe: bool,
    /// Last AB mover was a large piece (`is_big_piece`), capture or quiet.
    last_ab_mover_large: bool,
    /// Quiescence diagnostics (updated while in `quiesce`).
    q_nodes: u64,
    /// `q_nodes` at the start of the current root move.
    q_nodes_at_root_start: u64,
    /// Q-nodes spent on the most recently finished root move.
    q_nodes_last_root: u64,
    q_depth_left: u32,
    q_caps_at_node: usize,
    q_cap_index: usize,
    q_label: String,
    q_stand_pat: i32,
    q_prune_mode: QPruneMode,
    q_caps_generated: u64,
    q_caps_searched: u64,
    /// Searched quiescence captures by kind (PathAware diagnostics).
    q_kind_simple: u64,
    q_kind_path: u64,
    q_kind_multi: u64,
    /// Root PVS diagnostics.
    root_pvs_tried: u64,
    root_fail_high: u64,
    root_near_best: u64,
    root_moves_scored: u64,
    /// Unique q position hashes (capped).
    q_unique: HashSet<u64>,
    q_unique_saturated: bool,
    q_tt_hits: u64,
    q_tt_probes: u64,
    /// Instant when the current root index last advanced (for `rms=` logs).
    root_move_started: Instant,
    q_hash_prev_to: bool,
    q_no_pathclear_reply: bool,
    q_no_pathclear: bool,
    q_loud_promo_simple_only: bool,
    track_q_unique: bool,
    sibling_mode: u8,
    q_no_pathclear_after_wipe: bool,
    hang_q_dest_multileg: bool,
    hang_q_dest_pathclear: bool,
    q_open_large_mover: bool,
    q_open_any_capture: bool,
    q_recapture_only: bool,
    q_own_large_only: bool,
    sib_reduced: u64,
    sib_researched: u64,
}

impl QPruneMode {
    fn uses_net_gain(self) -> bool {
        matches!(
            self,
            Self::NetGainDelta | Self::NetGainAndTopN | Self::PathAware
        )
    }

    fn uses_top_n(self) -> bool {
        matches!(
            self,
            Self::TopN | Self::NetGainAndTopN | Self::TopNAndRecapture | Self::PathAware
        )
    }

    fn uses_recapture_only(self) -> bool {
        matches!(self, Self::RecaptureOnly | Self::TopNAndRecapture)
    }

    fn uses_stale_hang(self) -> bool {
        matches!(self, Self::StaleHang)
    }

    fn uses_path_aware(self) -> bool {
        matches!(self, Self::PathAware)
    }
}

type MoveKey = (u8, u8, u8, u8, bool);

#[derive(Clone, Copy, PartialEq, Eq)]
enum TtBound {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy)]
struct TtEntry {
    key: u64,
    depth: u32,
    score: i32,
    bound: TtBound,
    best: Option<MoveKey>, // from_file, from_rank, to_file, to_rank, promoted
}

struct TranspositionTable {
    entries: Vec<Option<TtEntry>>,
    clusters: usize,
}

impl TranspositionTable {
    fn new(size_pow2: usize) -> Self {
        Self::with_clusters(size_pow2, 1)
    }

    fn with_clusters(size_pow2: usize, clusters: usize) -> Self {
        let clusters = clusters.clamp(1, 8);
        let n = size_pow2.next_power_of_two().max(1024);
        let n = (n / clusters) * clusters;
        Self {
            entries: vec![None; n],
            clusters,
        }
    }

    fn set_base(&self, key: u64) -> usize {
        let n_sets = self.entries.len() / self.clusters;
        (key as usize & (n_sets - 1)) * self.clusters
    }

    fn probe(&self, key: u64) -> Option<&TtEntry> {
        let base = self.set_base(key);
        for i in 0..self.clusters {
            if let Some(e) = &self.entries[base + i] {
                if e.key == key {
                    return Some(e);
                }
            }
        }
        None
    }

    fn store(&mut self, entry: TtEntry) {
        let base = self.set_base(entry.key);
        if self.clusters == 1 {
            let replace = match &self.entries[base] {
                None => true,
                Some(old) => entry.depth >= old.depth || old.key != entry.key,
            };
            if replace {
                self.entries[base] = Some(entry);
            }
            return;
        }
        let mut empty = None;
        let mut worst = base;
        let mut worst_depth = u32::MAX;
        for i in 0..self.clusters {
            let idx = base + i;
            match &self.entries[idx] {
                None => {
                    empty = Some(idx);
                    break;
                }
                Some(old) if old.key == entry.key => {
                    if entry.depth >= old.depth {
                        self.entries[idx] = Some(entry);
                    }
                    return;
                }
                Some(old) => {
                    if old.depth < worst_depth {
                        worst_depth = old.depth;
                        worst = idx;
                    }
                }
            }
        }
        self.entries[empty.unwrap_or(worst)] = Some(entry);
    }
}

fn position_hash(state: &GameState) -> u64 {
    state.hash()
}

fn q_tt_key(state: &GameState, prev_to: Option<Position>, mix_prev_to: bool) -> u64 {
    let mut k = state.hash();
    if mix_prev_to {
        let salt = match prev_to {
            None => 0u64,
            Some(p) => 1 + p.to_index() as u64,
        };
        k ^= salt.wrapping_mul(0x9E3779B97F4A7C15);
    }
    k
}

fn tt_from_config(size_pow2: usize, config: &SearchConfig) -> TranspositionTable {
    TranspositionTable::with_clusters(size_pow2, config.tt_clusters.max(1) as usize)
}

fn move_tt_key(mv: &Move) -> MoveKey {
    (
        mv.from.file,
        mv.from.rank,
        mv.to.file,
        mv.to.rank,
        mv.promoted,
    )
}

fn same_tt_move(mv: &Move, key: MoveKey) -> bool {
    move_tt_key(mv) == key
}

fn history_key(mv: &Move) -> u32 {
    ((mv.from.to_index() as u32) << 16) | (mv.to.to_index() as u32)
}

fn ensure_killers(ctx: &mut SearchContext, ply: usize) {
    while ctx.killers.len() <= ply {
        ctx.killers.push([None, None]);
    }
}

fn store_killer(ctx: &mut SearchContext, ply: usize, key: MoveKey) {
    ensure_killers(ctx, ply);
    if ctx.killers[ply][0] == Some(key) {
        return;
    }
    ctx.killers[ply][1] = ctx.killers[ply][0];
    ctx.killers[ply][0] = Some(key);
}

fn killer_rank(ctx: &SearchContext, ply: usize, mv: &Move) -> i32 {
    let key = move_tt_key(mv);
    if ctx.killers.get(ply).and_then(|k| k[0]) == Some(key) {
        2
    } else if ctx.killers.get(ply).and_then(|k| k[1]) == Some(key) {
        1
    } else {
        0
    }
}

fn history_score(ctx: &SearchContext, mv: &Move) -> i32 {
    ctx.history.get(&history_key(mv)).copied().unwrap_or(0)
}

fn bump_history(ctx: &mut SearchContext, mv: &Move, depth: u32) {
    let d = depth as i32;
    let add = d.saturating_mul(d);
    let e = ctx.history.entry(history_key(mv)).or_insert(0);
    *e = e.saturating_add(add).min(1_000_000);
}

/// True if `mv` captures enemy material and is not a pure self-capture (`from == to`).
pub fn move_captures_enemy(state: &GameState, mv: &Move) -> bool {
    move_captures_enemy_raw(state, mv)
}

fn move_captures_enemy_raw(state: &GameState, mv: &Move) -> bool {
    if mv.from == mv.to {
        return false;
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return false;
    };
    let enemy = piece.color.opposite();

    if board.get_piece(mv.to).is_some_and(|p| p.color == enemy) {
        return true;
    }
    if let Some(inter) = mv.intermediate() {
        if board.get_piece(inter).is_some_and(|p| p.color == enemy) {
            return true;
        }
    }
    if let Some(path) = mv.free_eagle_path() {
        return path
            .iter()
            .skip(1)
            .any(|pos| board.get_piece(*pos).is_some_and(|p| p.color == enemy));
    }

    let config = MovementConfig::for_piece(&piece);
    let uses_capturing = config.capabilities.iter().any(|cap| {
        matches!(
            cap,
            MovementCapability::Range {
                blocking: BlockingMode::Capturing,
                ..
            }
        )
    });
    if uses_capturing {
        for pos in path_utils::get_path_positions(mv.from, mv.to) {
            if pos != mv.from
                && pos != mv.to
                && board.get_piece(pos).is_some_and(|p| p.color == enemy)
            {
                return true;
            }
        }
    }
    false
}

/// Enemy material taken vs own material destroyed by the move itself
/// (capturing-range / FE path clears). Does not model recapture of the mover.
fn capture_material_exchange(state: &GameState, weights: &EvalWeights, mv: &Move) -> (f32, f32) {
    let (enemy, own, _) = capture_exchange_kind(state, weights, mv);
    (enemy, own)
}

/// Exchange plus capture kind in one path walk (quiescence candidate cache).
fn capture_exchange_kind(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> (f32, f32, CaptureKind) {
    if mv.from == mv.to {
        return (0.0, 0.0, CaptureKind::SimpleTake);
    }
    if mv.is_two_step() || mv.is_free_eagle() {
        let (enemy, own) = capture_material_exchange_raw(state, weights, mv);
        return (enemy, own, CaptureKind::MultiLeg);
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return (0.0, 0.0, CaptureKind::SimpleTake);
    };
    let us = piece.color;
    let them = us.opposite();
    let mut enemy = 0.0f32;
    let mut own = 0.0f32;
    let mut path_occupied = false;

    let mut add = |pos: crate::position::Position| {
        if let Some(p) = board.get_piece(pos) {
            let v = material_piece_value(&p, weights);
            if p.color == them {
                enemy += v;
            } else if p.color == us {
                own += v;
            }
        }
    };

    add(mv.to);
    if let Some(inter) = mv.intermediate() {
        add(inter);
    }

    if !piece_has_capturing_range(&piece) {
        return (enemy, own, CaptureKind::SimpleTake);
    }
    for pos in path_utils::get_path_positions(mv.from, mv.to) {
        if pos != mv.from && pos != mv.to && board.get_piece(pos).is_some() {
            path_occupied = true;
            add(pos);
        }
    }
    let kind = if path_occupied {
        CaptureKind::PathClear
    } else {
        CaptureKind::SimpleTake
    };
    (enemy, own, kind)
}

/// Material exchange for multi-leg / FreeEagle (path clears on FE route).
fn capture_material_exchange_raw(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> (f32, f32) {
    if mv.from == mv.to {
        return (0.0, 0.0);
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return (0.0, 0.0);
    };
    let us = piece.color;
    let them = us.opposite();
    let mut enemy = 0.0f32;
    let mut own = 0.0f32;

    let mut add = |pos: crate::position::Position| {
        if let Some(p) = board.get_piece(pos) {
            let v = material_piece_value(&p, weights);
            if p.color == them {
                enemy += v;
            } else if p.color == us {
                own += v;
            }
        }
    };

    add(mv.to);
    if let Some(inter) = mv.intermediate() {
        add(inter);
    }
    if let Some(path) = mv.free_eagle_path() {
        for pos in path.iter().skip(1) {
            if *pos != mv.to {
                add(*pos);
            }
        }
        return (enemy, own);
    }

    if piece_has_capturing_range(&piece) {
        for pos in path_utils::get_path_positions(mv.from, mv.to) {
            if pos != mv.from && pos != mv.to {
                add(pos);
            }
        }
    }
    (enemy, own)
}

/// Minimum enemy material for a capture to enter quiescence (capability scale).
/// Derived from range tariffs via [`seed_loud_capture_floor`].
pub fn min_quiescence_enemy_material() -> f32 {
    seed_loud_capture_floor()
}

/// True when net exchange is too small vs mover value to risk a hanging landing.
fn net_below_hang_frac(enemy: f32, own: f32, mover_value: f32) -> bool {
    (enemy - own) < mover_value * HANG_NET_FRAC
}

/// Dest, intermediate, capturing-range path, or Free Eagle route takes an enemy King / CP.
/// Hang-skip must not drop these: royals are cheap in material (CP=8) but end the game.
fn capture_takes_enemy_royal(state: &GameState, mv: &Move) -> bool {
    captured_enemy_royal_count(state, mv) > 0
}

/// True when this capture removes every remaining enemy royal (instant win).
fn capture_takes_last_enemy_royal(state: &GameState, mv: &Move) -> bool {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return false;
    };
    let them = mover.color.opposite();
    let have = board
        .iter_pieces_by_color(them)
        .filter(|p| p.piece_type.is_royal())
        .count();
    have > 0 && captured_enemy_royal_count(state, mv) >= have
}

fn captured_enemy_royal_count(state: &GameState, mv: &Move) -> usize {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return 0;
    };
    let them = mover.color.opposite();
    let enemy_royal = |pos: Position| {
        board
            .get_piece(pos)
            .is_some_and(|p| p.color == them && p.piece_type.is_royal())
    };
    let mut n = 0usize;
    let mut seen = [None; 4];
    let mut add = |pos: Position| {
        if enemy_royal(pos) && !seen[..n].contains(&Some(pos)) && n < seen.len() {
            seen[n] = Some(pos);
            n += 1;
        }
    };
    add(mv.to);
    if let Some(inter) = mv.intermediate() {
        add(inter);
    }
    if let Some(path) = mv.free_eagle_path() {
        for &pos in path {
            add(pos);
        }
    } else if piece_has_capturing_range(&mover) {
        for pos in path_utils::get_path_positions(mv.from, mv.to) {
            if pos != mv.from && pos != mv.to {
                add(pos);
            }
        }
    }
    n
}

/// True when a capture hangs a high-value mover and should be skipped in AB.
///
/// Conditions: mover value ≥ [`HIGH_VALUE_HANGER`], net below [`HANG_NET_FRAC`] of
/// mover, and landing attacked.
///
/// `SimpleTake`: cheap pre-move landing attack.
/// `PathClear` / `MultiLeg`: cheap pre-move first; only if that looks hanging,
/// confirm with post-fire simulation. Captured path pieces often "defend" the
/// landing pre-move and vanish after the clear — skipping the confirm false-prunes
/// free takes (e.g. GG path-clearing a Hook Mover then landing safely).
///
/// `postfire_pathclear_hang`: when true, PathClear/MultiLeg go straight to post-fire
/// (root). When false, use confirm-on-prune (interior).
fn capture_hangs_high_value_piece(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    postfire_pathclear_hang: bool,
    attack_cache: &mut LandingAttackCache,
) -> bool {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return false;
    };
    let mover_value = material_piece_value(&mover, weights);
    if mover_value < HIGH_VALUE_HANGER {
        return false;
    }
    if capture_takes_enemy_royal(state, mv) {
        return false;
    }
    let (enemy, own, kind) = capture_exchange_kind(state, weights, mv);
    if enemy == 0.0 || !net_below_hang_frac(enemy, own, mover_value) {
        return false;
    }
    let opponent = state.get_current_turn().opposite();
    match kind {
        CaptureKind::SimpleTake => landing_attacked_cached(board, mv.to, opponent, attack_cache),
        CaptureKind::PathClear | CaptureKind::MultiLeg => {
            if !postfire_pathclear_hang
                && !landing_attacked_cached(board, mv.to, opponent, attack_cache)
            {
                return false;
            }
            let vb = crate::move_simulation::simulate_move(board, mv, &mover);
            vb.is_position_attacked_by_color(mv.to, opponent)
        }
    }
}

/// Quiescence candidate: worthwhile captures, or recapture onto the previous landing.
pub fn is_quiescence_capture_candidate(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    prev_to: Option<Position>,
) -> bool {
    if !move_captures_enemy_raw(state, mv) {
        return false;
    }
    if is_worthwhile_quiescence_capture(state, weights, mv) {
        return true;
    }
    prev_to
        .map(|sq| capture_hits_square(state, mv, sq))
        .unwrap_or(false)
}

/// Quiescence only expands "loud" captures (big enemy material), not nibbling
/// at low-value pieces. Pure self-captures excluded.
pub fn is_worthwhile_quiescence_capture(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> bool {
    if !move_captures_enemy_raw(state, mv) {
        return false;
    }
    if capture_takes_enemy_royal(state, mv) {
        return true;
    }
    let (enemy, _own) = capture_material_exchange(state, weights, mv);
    enemy >= min_quiescence_enemy_material()
}

/// Promotion that creates a two-mover / range-capturer (e.g. FreeKing→GreatGeneral).
pub fn is_loud_promotion_move(state: &GameState, mv: &Move) -> bool {
    if !mv.promoted {
        return false;
    }
    let Some(piece) = state.get_board().get_piece(mv.from) else {
        return false;
    };
    if piece.is_promoted {
        return false;
    }
    promotes_into_big_piece(piece.piece_type)
}

/// Material gain from the promotion itself (promoted type − base), else 0.
pub fn loud_promotion_material_gain(state: &GameState, weights: &EvalWeights, mv: &Move) -> f32 {
    if !mv.promoted {
        return 0.0;
    }
    let Some(piece) = state.get_board().get_piece(mv.from) else {
        return 0.0;
    };
    let Some(to_pt) = piece.piece_type.promotes_to() else {
        return 0.0;
    };
    if !is_big_piece(to_pt) {
        return 0.0;
    }
    weights.piece_value(to_pt) - weights.piece_value(piece.piece_type)
}

/// AB / leaf gate strength for a move: capture victim value or loud-promo gain.
fn move_loudness(state: &GameState, weights: &EvalWeights, mv: &Move, is_capture: bool) -> f32 {
    let cap = if is_capture {
        capture_material_exchange(state, weights, mv).0
    } else {
        0.0
    };
    cap.max(loud_promotion_material_gain(state, weights, mv))
}

/// Legal promotions into two-movers / range capturers (quiet or capturing).
pub fn generate_loud_promotions(state: &GameState) -> Vec<Move> {
    let us = state.get_current_turn();
    let mut movers = Vec::new();
    for p in state.get_board().iter_pieces_by_color(us) {
        if !piece_might_loud_promote(&p) {
            continue;
        }
        movers.push(p);
    }
    if movers.is_empty() {
        return Vec::new();
    }
    let mut generated = Vec::new();
    state.generate_legal_moves_for_pieces_mode(&movers, LegalMoveGen::All, &mut generated);
    generated.into_iter().filter(|mv| mv.promoted).collect()
}

/// Conservative: true if this piece can legally promote into a big type this turn.
fn piece_might_loud_promote(piece: &Piece) -> bool {
    if piece.is_promoted || !promotes_into_big_piece(piece.piece_type) {
        return false;
    }
    let reach = max_rank_delta_toward_promo(piece);
    match piece.color {
        Color::Black => {
            piece.position.rank >= 25 || 25u8.saturating_sub(piece.position.rank) <= reach
        }
        Color::White => {
            piece.position.rank <= 10 || piece.position.rank.saturating_sub(10) <= reach
        }
    }
}

fn max_rank_delta_toward_promo(piece: &Piece) -> u8 {
    let mut max = 0u8;
    for cap in &MovementConfig::for_piece(piece).capabilities {
        match cap {
            MovementCapability::Range { .. }
            | MovementCapability::TwoStep { .. }
            | MovementCapability::FreeEagleMultiMove { .. }
            | MovementCapability::ConditionalDiagonalJump { .. } => return 36,
            MovementCapability::Simple { max_distance, .. } => {
                max = max.max(*max_distance);
            }
            MovementCapability::Jumping { offsets } => {
                for &(_, dr) in offsets {
                    max = max.max(dr.unsigned_abs());
                }
            }
        }
    }
    max
}

/// MVV-LVA capture score without hang checks (for quiescence ordering).
fn mvv_lva_score(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    let mover_value = material_piece_value(&mover, weights);
    let (enemy, own) = capture_material_exchange(state, weights, mv);
    if enemy == 0.0 {
        return i32::MIN / 4;
    }
    ((enemy - own) * 1000.0 - mover_value).round() as i32
}

/// Move-ordering score (heuristic only — not search correctness).
///
/// Captures: `gain = enemy - own`. SimpleTake uses a cheap pre-move landing
/// attack cache. PathClear / MultiLeg use confirm-on-prune (cheap pre-move, then
/// post-fire if that looks hanging). When `postfire_pathclear_hang` is set, those
/// kinds go straight to post-fire (root). LVA: `gain*1000 - mover`.
fn move_order_score(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    opponent: Color,
    attack_cache: &mut LandingAttackCache,
    postfire_pathclear_hang: bool,
) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    if capture_takes_last_enemy_royal(state, mv) {
        return LAST_ROYAL_ORDER;
    }
    let mover_value = material_piece_value(&mover, weights);
    let (enemy, own, kind) = capture_exchange_kind(state, weights, mv);
    if enemy == 0.0 {
        return i32::MIN / 4;
    }

    let mut gain = enemy - own;
    let hanging =
        if !capture_takes_enemy_royal(state, mv) && net_below_hang_frac(enemy, own, mover_value) {
            match kind {
                CaptureKind::SimpleTake => {
                    landing_attacked_cached(board, mv.to, opponent, attack_cache)
                }
                CaptureKind::PathClear | CaptureKind::MultiLeg => {
                    // Same confirm-on-prune as [`capture_hangs_high_value_piece`].
                    if !postfire_pathclear_hang
                        && !landing_attacked_cached(board, mv.to, opponent, attack_cache)
                    {
                        false
                    } else {
                        let vb = crate::move_simulation::simulate_move(board, mv, &mover);
                        vb.is_position_attacked_by_color(mv.to, opponent)
                    }
                }
            }
        } else {
            false
        };
    if hanging {
        gain -= mover_value;
    }
    (gain * 1000.0 - mover_value).round() as i32
}

/// Per-node landing-attack memo (36×36). Avoids a HashMap alloc on every order.
struct LandingAttackCache {
    /// 0 = unknown, 1 = safe, 2 = attacked.
    state: [u8; 1296],
}

impl LandingAttackCache {
    fn new() -> Self {
        Self { state: [0; 1296] }
    }
}

fn landing_attacked_cached(
    board: &crate::board::Board,
    to: Position,
    opponent: Color,
    cache: &mut LandingAttackCache,
) -> bool {
    let idx = to.to_index();
    match cache.state[idx] {
        1 => return false,
        2 => return true,
        _ => {}
    }
    let hit = board.is_position_attacked_by_color(to, opponent);
    cache.state[idx] = if hit { 2 } else { 1 };
    hit
}

fn config_has_capturing_range(caps: &[MovementCapability]) -> bool {
    caps.iter().any(|cap| {
        matches!(
            cap,
            MovementCapability::Range {
                blocking: BlockingMode::Capturing,
                ..
            }
        )
    })
}

fn capturing_range_table() -> &'static [[bool; 2]] {
    static TABLE: OnceLock<Vec<[bool; 2]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut max_idx = 0usize;
        for &pt in crate::eval::ALL_PIECE_TYPES {
            max_idx = max_idx.max(pt as usize);
        }
        let mut t = vec![[false; 2]; max_idx + 1];
        for &pt in crate::eval::ALL_PIECE_TYPES {
            t[pt as usize][0] =
                config_has_capturing_range(&MovementConfig::for_piece_type(pt).capabilities);
            let mut promo = Piece::new(pt, Color::Black, Position::new(0, 0).unwrap());
            promo.is_promoted = true;
            t[pt as usize][1] =
                config_has_capturing_range(&MovementConfig::for_piece(&promo).capabilities);
        }
        t
    })
}

fn piece_has_capturing_range(piece: &Piece) -> bool {
    if piece.base_piece_type.is_some() {
        return config_has_capturing_range(&MovementConfig::for_piece(piece).capabilities);
    }
    capturing_range_table()
        .get(piece.piece_type as usize)
        .map(|row| row[piece.is_promoted as usize])
        .unwrap_or_else(|| {
            config_has_capturing_range(&MovementConfig::for_piece(piece).capabilities)
        })
}

/// True if this capture is a capturing-range path clear or multi-leg (FE / two-step).
/// Cheap structural check — no piece-value walk.
fn quiesce_move_looks_path_or_multileg(state: &GameState, mv: &Move) -> bool {
    if mv.is_two_step() || mv.is_free_eagle() {
        return true;
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return false;
    };
    if !piece_has_capturing_range(&piece) {
        return false;
    }
    path_utils::get_path_positions(mv.from, mv.to)
        .into_iter()
        .any(|p| p != mv.from && p != mv.to && board.get_piece(p).is_some())
}

/// Dest-hang q gates. Production default is both on (A+B).
#[derive(Debug, Clone, Copy)]
struct QHangOpts {
    dest_multileg: bool,
    dest_pathclear: bool,
}

impl QHangOpts {
    const OFF: Self = Self {
        dest_multileg: false,
        dest_pathclear: false,
    };
    const AB: Self = Self {
        dest_multileg: true,
        dest_pathclear: true,
    };

    fn from_ctx(ctx: &SearchContext) -> Self {
        Self {
            dest_multileg: ctx.hang_q_dest_multileg,
            dest_pathclear: ctx.hang_q_dest_pathclear,
        }
    }

    fn any(self) -> bool {
        self.dest_multileg || self.dest_pathclear
    }
}

impl Default for QHangOpts {
    fn default() -> Self {
        Self::AB
    }
}

/// Dest-capture of a hanging large enemy, optionally including MultiLeg / PathClear.
///
/// SimpleTakes of ordinary heavies still need a cheaper attacker (equal GG
/// trades must not open q). A dest take of a range two-mover counts for any
/// attacker — hook-takes-hook is the same hanging piece whether the mover
/// is a Peacock or the other Hook.
fn dest_hang_kind(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    opts: QHangOpts,
) -> Option<CaptureKind> {
    if mv.to == mv.from {
        return None;
    }
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return None;
    };
    let Some(victim) = board.get_piece(mv.to) else {
        return None;
    };
    if victim.color == mover.color || !is_large_hang_victim(&victim, weights) {
        return None;
    }
    if !is_range_two_mover(victim.piece_type)
        && material_piece_value(&mover, weights) >= material_piece_value(&victim, weights)
    {
        return None;
    }
    let (_, _, kind) = capture_exchange_kind(state, weights, mv);
    match kind {
        CaptureKind::SimpleTake => Some(CaptureKind::SimpleTake),
        CaptureKind::MultiLeg => {
            if opts.dest_multileg && is_range_two_mover(victim.piece_type) {
                Some(CaptureKind::MultiLeg)
            } else {
                None
            }
        }
        CaptureKind::PathClear => {
            if opts.dest_pathclear && is_range_two_mover(victim.piece_type) {
                Some(CaptureKind::PathClear)
            } else {
                None
            }
        }
    }
}

fn generate_hang_dest_takes(
    state: &GameState,
    weights: &EvalWeights,
    opts: QHangOpts,
) -> Vec<Move> {
    if !opts.any() {
        return Vec::new();
    }
    let them = state.get_current_turn().opposite();
    let mut out = Vec::new();
    let mut seen: HashSet<(u16, u16, bool)> = HashSet::new();
    for enemy in state.get_board().iter_pieces_by_color(them) {
        if !is_range_two_mover(enemy.piece_type) || !is_large_hang_victim(&enemy, weights) {
            continue;
        }
        for mv in generate_captures_hitting_square(state, enemy.position) {
            if mv.to != enemy.position {
                continue;
            }
            if dest_hang_kind(state, weights, &mv, opts)
                .is_none_or(|k| matches!(k, CaptureKind::SimpleTake))
            {
                continue;
            }
            let key = (
                mv.from.to_index() as u16,
                mv.to.to_index() as u16,
                mv.promoted,
            );
            if seen.insert(key) {
                out.push(mv);
            }
        }
    }
    out
}

/// Dest (or two-step / path) captures of an enemy King or Crown Prince.
///
/// Royals are cheap in material (King 100, CP 8) so they fail the loud floor and
/// `is_big_piece`. A two-step Hook take of the king is still **one move / one q
/// ply** — both legs apply in `make_move_for_search`.
fn generate_royal_captures(state: &GameState) -> Vec<Move> {
    let them = state.get_current_turn().opposite();
    let mut out = Vec::new();
    let mut seen: HashSet<(u16, u16, bool)> = HashSet::new();
    for enemy in state.get_board().iter_pieces_by_color(them) {
        if !enemy.piece_type.is_royal() {
            continue;
        }
        for mv in generate_captures_hitting_square(state, enemy.position) {
            if !capture_takes_enemy_royal(state, &mv) {
                continue;
            }
            let key = (
                mv.from.to_index() as u16,
                mv.to.to_index() as u16,
                mv.promoted,
            );
            if seen.insert(key) {
                out.push(mv);
            }
        }
    }
    out
}

fn stm_has_royal_capture(state: &GameState) -> bool {
    let them = state.get_current_turn().opposite();
    for enemy in state.get_board().iter_pieces_by_color(them) {
        if !enemy.piece_type.is_royal() {
            continue;
        }
        for mv in generate_captures_hitting_square(state, enemy.position) {
            if capture_takes_enemy_royal(state, &mv) {
                return true;
            }
        }
    }
    false
}

fn mover_is_large(state: &GameState, mv: &Move) -> bool {
    state
        .get_board()
        .get_piece(mv.from)
        .is_some_and(|p| is_big_piece(p.piece_type))
}

/// Dest recapture of the previous landing when that piece is a large enemy.
fn stm_has_dest_take_of_prev_large(state: &GameState, prev_to: Option<Position>) -> bool {
    let Some(sq) = prev_to else {
        return false;
    };
    let them = state.get_current_turn().opposite();
    let Some(piece) = state.get_board().get_piece(sq) else {
        return false;
    };
    if piece.color != them || !is_big_piece(piece.piece_type) {
        return false;
    }
    generate_captures_hitting_square(state, sq)
        .iter()
        .any(|mv| mv.to == sq)
}

fn dest_victim_is_big(state: &GameState, mv: &Move) -> bool {
    let them = state.get_current_turn().opposite();
    state
        .get_board()
        .get_piece(mv.to)
        .is_some_and(|p| p.color == them && is_big_piece(p.piece_type))
}

/// Captures worth expanding in quiescence, plus promotions into big pieces.
///
/// Contract: q finishes the contested square (loud SimpleTakes + recaptures),
/// and also expands FreeKing→GG-style promotions that swing material massively.
/// Capturing-range corridor wipes / multi-leg snipes belong to main search unless
/// they are a destination recapture onto `prev_to`, a dest-hang of a range
/// two-mover, or they capture a royal (King/CP; two-step dest is one q ply).
///
/// - Deep PathAware (`victim_square_only`): only captures hitting `prev_to`, plus
///   loud promotions.
/// - Entry with `prev_to`: dest hits on `prev_to` plus loud SimpleTakes (no full-board
///   CapturesOnly), plus loud promotions.
/// - Entry without `prev_to`: full CapturesOnly fallback + loud promotions.
/// - `captures`: when false, loud promotions **and** royal takes (leaf after quiet AB).
fn generate_quiescence_captures(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Option<Position>,
    victim_square_only: bool,
    captures: bool,
    allow_pathclear: bool,
    loud_promo_simple_only: bool,
) -> Vec<Move> {
    generate_quiescence_captures_with_hang(
        state,
        weights,
        prev_to,
        victim_square_only,
        captures,
        allow_pathclear,
        loud_promo_simple_only,
        QHangOpts::default(),
    )
}

fn generate_quiescence_captures_with_hang(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Option<Position>,
    victim_square_only: bool,
    captures: bool,
    allow_pathclear: bool,
    loud_promo_simple_only: bool,
    hang: QHangOpts,
) -> Vec<Move> {
    let mut raw = if !captures {
        Vec::new()
    } else if victim_square_only {
        if let Some(victim) = prev_to {
            generate_captures_hitting_square(state, victim)
        } else {
            state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
        }
    } else if let Some(victim) = prev_to {
        generate_entry_quiescence_captures(state, weights, victim)
    } else {
        state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
    };
    let promos = generate_loud_promotions(state);
    if !promos.is_empty() {
        let mut seen: HashSet<(u16, u16, bool)> = raw
            .iter()
            .map(|mv| {
                (
                    mv.from.to_index() as u16,
                    mv.to.to_index() as u16,
                    mv.promoted,
                )
            })
            .collect();
        for mv in promos {
            let key = (
                mv.from.to_index() as u16,
                mv.to.to_index() as u16,
                mv.promoted,
            );
            if seen.insert(key) {
                raw.push(mv);
            }
        }
    }
    // Entry only: dest-hang MultiLeg/PathClear even when prev_to is not the victim.
    if captures && !victim_square_only && hang.any() {
        let extra = generate_hang_dest_takes(state, weights, hang);
        if !extra.is_empty() {
            let mut seen: HashSet<(u16, u16, bool)> = raw
                .iter()
                .map(|mv| {
                    (
                        mv.from.to_index() as u16,
                        mv.to.to_index() as u16,
                        mv.promoted,
                    )
                })
                .collect();
            for mv in extra {
                let key = (
                    mv.from.to_index() as u16,
                    mv.to.to_index() as u16,
                    mv.promoted,
                );
                if seen.insert(key) {
                    raw.push(mv);
                }
            }
        }
    }
    // Royal takes (King/CP), including two-step dest, even when prev_to is a
    // different square (slot0240: quiet Peacock, Hook mates on 18,35).
    {
        let extra = generate_royal_captures(state);
        if !extra.is_empty() {
            let mut seen: HashSet<(u16, u16, bool)> = raw
                .iter()
                .map(|mv| {
                    (
                        mv.from.to_index() as u16,
                        mv.to.to_index() as u16,
                        mv.promoted,
                    )
                })
                .collect();
            for mv in extra {
                let key = (
                    mv.from.to_index() as u16,
                    mv.to.to_index() as u16,
                    mv.promoted,
                );
                if seen.insert(key) {
                    raw.push(mv);
                }
            }
        }
    }
    if !captures {
        return raw;
    }
    raw.into_iter()
        .filter(|mv| {
            if capture_takes_enemy_royal(state, mv) {
                return true;
            }
            if dest_hang_kind(state, weights, mv, hang)
                .is_some_and(|k| !matches!(k, CaptureKind::SimpleTake))
            {
                return true;
            }
            if is_loud_promotion_move(state, mv) {
                if !(loud_promo_simple_only && quiesce_move_looks_path_or_multileg(state, mv)) {
                    return true;
                }
            }
            if !is_quiescence_capture_candidate(state, weights, mv, prev_to) {
                return false;
            }
            // Same policy as PathAware keep: PathClear/MultiLeg only as dest recapture.
            if quiesce_move_looks_path_or_multileg(state, mv)
                && (!allow_pathclear || prev_to != Some(mv.to))
            {
                return false;
            }
            true
        })
        .collect()
}

/// Q-entry without full-board CapturesOnly: dest hits on `prev_to` + loud SimpleTakes.
fn generate_entry_quiescence_captures(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Position,
) -> Vec<Move> {
    let mut out = generate_captures_hitting_square(state, prev_to);
    let mut seen: HashSet<(u16, u16, bool)> = out
        .iter()
        .map(|mv| {
            (
                mv.from.to_index() as u16,
                mv.to.to_index() as u16,
                mv.promoted,
            )
        })
        .collect();
    for mv in generate_loud_simple_takes(state, weights) {
        let key = (
            mv.from.to_index() as u16,
            mv.to.to_index() as u16,
            mv.promoted,
        );
        if seen.insert(key) {
            out.push(mv);
        }
    }
    out
}

/// Loud SimpleTakes: dest-capture each enemy piece valued ≥ the loud floor.
fn generate_loud_simple_takes(state: &GameState, weights: &EvalWeights) -> Vec<Move> {
    let floor = min_quiescence_enemy_material();
    let them = state.get_current_turn().opposite();
    let mut out = Vec::new();
    let mut seen: HashSet<(u16, u16, bool)> = HashSet::new();
    for enemy in state.get_board().iter_pieces_by_color(them) {
        if material_piece_value(&enemy, weights) < floor {
            continue;
        }
        for mv in generate_captures_hitting_square(state, enemy.position) {
            if mv.to != enemy.position {
                continue;
            }
            if quiesce_move_looks_path_or_multileg(state, &mv) {
                continue;
            }
            let key = (
                mv.from.to_index() as u16,
                mv.to.to_index() as u16,
                mv.promoted,
            );
            if seen.insert(key) {
                out.push(mv);
            }
        }
    }
    out
}

/// Large enemy piece eligible for hanging-take quiescence (big type or ≥ loud floor).
fn is_large_hang_victim(piece: &crate::piece::Piece, weights: &EvalWeights) -> bool {
    is_big_piece(piece.piece_type)
        || material_piece_value(piece, weights) >= min_quiescence_enemy_material()
}

/// True when `mv` is a SimpleTake of a large enemy by a strictly cheaper mover.
pub(crate) fn is_large_hang_simple_take(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> bool {
    if quiesce_move_looks_path_or_multileg(state, mv) {
        return false;
    }
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return false;
    };
    let Some(victim) = board.get_piece(mv.to) else {
        return false;
    };
    if victim.color == mover.color || !is_large_hang_victim(&victim, weights) {
        return false;
    }
    material_piece_value(&mover, weights) < material_piece_value(&victim, weights)
}

/// STM can SimpleTake a large hanging enemy with a lesser-valued piece.
///
/// Used to open quiescence after quiet AB parents (classical engines resolve
/// free hanging heavies in q; we previously stand-pat and missed them).
pub(crate) fn stm_has_large_hang_simple_take(state: &GameState, weights: &EvalWeights) -> bool {
    stm_has_large_hang_take(state, weights, QHangOpts::OFF)
}

fn stm_has_large_hang_take(state: &GameState, weights: &EvalWeights, opts: QHangOpts) -> bool {
    let them = state.get_current_turn().opposite();
    for enemy in state.get_board().iter_pieces_by_color(them) {
        if !is_large_hang_victim(&enemy, weights) {
            continue;
        }
        for mv in generate_captures_hitting_square(state, enemy.position) {
            if mv.to != enemy.position {
                continue;
            }
            if dest_hang_kind(state, weights, &mv, opts).is_some() {
                return true;
            }
        }
    }
    false
}

fn capability_is_directed_ok(cap: &MovementCapability) -> bool {
    matches!(
        cap,
        MovementCapability::Simple { .. }
            | MovementCapability::Range { .. }
            | MovementCapability::Jumping { .. }
    )
}

/// TwoStep pieces whose legs are Simple/Range/Jumping can emit victim-directed captures.
fn can_directed_two_step_victim_hits(capabilities: &[MovementCapability]) -> bool {
    let mut has_two_step = false;
    for cap in capabilities {
        match cap {
            MovementCapability::FreeEagleMultiMove { .. }
            | MovementCapability::ConditionalDiagonalJump { .. } => return false,
            MovementCapability::TwoStep { first, second } => {
                has_two_step = true;
                if !capability_is_directed_ok(first) || !capability_is_directed_ok(second) {
                    return false;
                }
            }
            _ => {}
        }
    }
    has_two_step
}

fn emit_directed_two_step_captures_hitting(
    state: &GameState,
    piece: &crate::piece::Piece,
    victim: Position,
    capabilities: &[MovementCapability],
    out: &mut Vec<Move>,
) {
    let board = state.get_board();
    let victim_is_enemy = board
        .get_piece(victim)
        .is_some_and(|p| p.color != piece.color);
    for cap in capabilities {
        match cap {
            MovementCapability::TwoStep { first, second } => {
                for landing in MovementGenerator::capture_landings_hitting_target(
                    piece,
                    board,
                    std::slice::from_ref(first.as_ref()),
                    victim,
                ) {
                    state.emit_standard_moves_to(piece, landing, out);
                }
                let first_targets =
                    MovementGenerator::capability_landings(piece, board, first.as_ref());
                for intermediate in first_targets {
                    let mut temp = *piece;
                    temp.position = intermediate;
                    if victim_is_enemy && intermediate == victim {
                        for target in MovementGenerator::generate_targets_filtered(
                            &temp,
                            board,
                            std::slice::from_ref(second.as_ref()),
                            false,
                        ) {
                            state.emit_two_step_moves_to(piece, intermediate, target, out);
                        }
                    } else {
                        for landing in MovementGenerator::capture_landings_hitting_target(
                            &temp,
                            board,
                            std::slice::from_ref(second.as_ref()),
                            victim,
                        ) {
                            state.emit_two_step_moves_to(piece, intermediate, landing, out);
                        }
                    }
                }
            }
            _ => {
                for landing in MovementGenerator::capture_landings_hitting_target(
                    piece,
                    board,
                    std::slice::from_ref(cap),
                    victim,
                ) {
                    state.emit_standard_moves_to(piece, landing, out);
                }
            }
        }
    }
}

fn append_full_gen_hits(state: &GameState, piece: Piece, victim: Position, out: &mut Vec<Move>) {
    let start = out.len();
    state.generate_legal_moves_for_pieces_mode(&[piece], LegalMoveGen::CapturesOnly, out);
    let mut w = start;
    for r in start..out.len() {
        if capture_hits_square(state, &out[r], victim) {
            out.swap(w, r);
            w += 1;
        }
    }
    out.truncate(w);
}

/// Captures that take an enemy on `victim` (dest, path-clear, multi-leg, FE).
///
/// Standard pieces use directed landing emit; TwoStep uses directed first/second
/// legs when both are Simple/Range/Jumping. FreeEagle / conditional-jump fall
/// back to per-piece CapturesOnly + filter (parity-gated via [`crate::parity`]).
pub(crate) fn generate_captures_hitting_square(state: &GameState, victim: Position) -> Vec<Move> {
    #[cfg(feature = "search-profile")]
    let _prof = crate::profile_timers::gen_scope();
    let us = state.get_current_turn();
    let board = state.get_board();
    let mut out = Vec::new();
    for piece in board.iter_pieces_by_color(us) {
        if !crate::attack_utils::should_check_piece_for_target_position(&piece, victim, false) {
            continue;
        }
        if piece.piece_type == crate::piece::PieceType::FreeEagle {
            append_full_gen_hits(state, piece, victim, &mut out);
            continue;
        }
        let config = MovementConfig::for_piece(&piece);
        if MovementGenerator::needs_full_gen_for_victim_hits(&config.capabilities) {
            if can_directed_two_step_victim_hits(&config.capabilities) {
                #[cfg(feature = "search-profile")]
                let _ts = crate::profile_timers::two_step_scope();
                emit_directed_two_step_captures_hitting(
                    state,
                    &piece,
                    victim,
                    &config.capabilities,
                    &mut out,
                );
            } else {
                append_full_gen_hits(state, piece, victim, &mut out);
            }
            continue;
        }
        {
            #[cfg(feature = "search-profile")]
            let _std = crate::profile_timers::standard_gen_scope();
            for landing in MovementGenerator::capture_landings_hitting_target(
                &piece,
                board,
                &config.capabilities,
                victim,
            ) {
                state.emit_standard_moves_to(&piece, landing, &mut out);
            }
        }
    }
    out
}

/// Pick a move with alpha-beta (no GUI trace by default).
///
/// Uses iterative deepening from depth 1..=`config.depth`. On timeout mid-iteration,
/// returns the last **completed** iteration's result.
pub fn search(state: &GameState, weights: &EvalWeights, config: &SearchConfig) -> SearchResult {
    // Search eval skips deterministic noise (hashes every piece when enabled).
    let mut weights_buf;
    let weights = if weights.noise_scale != 0.0 {
        weights_buf = weights.clone();
        weights_buf.noise_scale = 0.0;
        &weights_buf
    } else {
        weights
    };
    let _wbind = bind_search_weights(weights);

    let root_ply = state.get_move_history().len();
    let static_eval = evaluate_with_ply(state, weights, root_ply);
    let deadline = config
        .max_time_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));
    let now = Instant::now();
    let max_depth = config.depth.max(1);

    let mut ctx = SearchContext {
        deadline,
        nodes: 0,
        abort: false,
        ply: root_ply,
        quiescence_depth: config.quiescence_depth,
        quiesce_entry_depth: config.quiescence_depth,
        search_started: now,
        last_progress_log: now,
        search_depth: max_depth,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "root",
        tt: tt_from_config(1 << 20, config),
        q_tt: tt_from_config(1 << 18, config),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy: 0.0,
        last_ab_to: None,
        last_ab_wipe: false,
        last_ab_mover_large: false,
        q_nodes: 0,
        q_nodes_at_root_start: 0,
        q_nodes_last_root: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: config.q_prune_mode,
        q_caps_generated: 0,
        q_caps_searched: 0,
        q_kind_simple: 0,
        q_kind_path: 0,
        q_kind_multi: 0,
        root_pvs_tried: 0,
        root_fail_high: 0,
        root_near_best: 0,
        root_moves_scored: 0,
        q_unique: HashSet::new(),
        q_unique_saturated: false,
        q_tt_hits: 0,
        q_tt_probes: 0,
        root_move_started: now,
        q_hash_prev_to: config.q_hash_prev_to,
        q_no_pathclear_reply: config.q_no_pathclear_reply,
        q_no_pathclear: config.q_no_pathclear,
        q_loud_promo_simple_only: config.q_loud_promo_simple_only,
        track_q_unique: config.track_q_unique,
        sibling_mode: config.sibling_mode,
        q_no_pathclear_after_wipe: config.q_no_pathclear_after_wipe,
        hang_q_dest_multileg: config.hang_q_dest_multileg,
        hang_q_dest_pathclear: config.hang_q_dest_pathclear,
        q_open_large_mover: config.q_open_large_mover,
        q_open_any_capture: config.q_open_any_capture,
        q_recapture_only: config.q_recapture_only,
        q_own_large_only: config.q_own_large_only,
        sib_reduced: 0,
        sib_researched: 0,
    };

    let mut moves = state.generate_legal_moves();
    if moves.is_empty() {
        let tree = SearchTreeNode {
            label: "root".into(),
            score: Some(static_eval),
            static_eval: Some(static_eval),
            best: true,
            cutoff: false,
            children: vec![],
        };
        return SearchResult {
            best_move: None,
            score: static_eval,
            nodes: 0,
            static_eval,
            root_lines: vec![],
            tree,
            q_nodes: 0,
            q_caps_generated: 0,
            q_caps_searched: 0,
            q_tt_hits: 0,
            q_tt_probes: 0,
            q_kind_path: 0,
            q_kind_simple: 0,
            q_kind_multi: 0,
            q_unique: 0,
            q_unique_saturated: false,
            root_moves_scored: 0,
            aborted: false,
        };
    }

    order_moves_with_heuristics(state, weights, &mut moves, &ctx, root_ply, false, true);
    ctx.root_total = moves.len();

    let mut completed_best = moves[0].clone();
    let mut completed_score = i32::MIN + 1;
    let mut completed_lines: Vec<(Move, i32)> = Vec::new();
    let mut completed_depth = 0u32;

    // One working copy for the whole ID loop; make/unmake instead of per-child clone.
    let mut pos = state.clone();
    pos.ensure_eval_inc(weights);

    for d in 1..=max_depth {
        if ctx.timed_out() {
            break;
        }
        ctx.search_depth = d;
        ctx.phase = "root";
        ctx.best_score = completed_score;
        ctx.root_total = moves.len();

        let mut iter_best = moves[0].clone();
        let mut iter_score = i32::MIN + 1;
        let mut alpha = i32::MIN + 1;
        let beta = i32::MAX - 1;
        let mut iter_lines: Vec<(Move, i32)> = Vec::with_capacity(moves.len());
        let mut finished_iteration = true;
        let mut sib_reps: HashMap<u64, (i32, i32)> = HashMap::new();

        for (i, mv) in moves.iter().enumerate() {
            if ctx.timed_out() {
                finished_iteration = false;
                break;
            }
            ctx.root_index = i + 1;
            ctx.root_label = move_label(state, mv);
            ctx.phase = "root";
            ctx.q_nodes_at_root_start = ctx.q_nodes;
            ctx.root_move_started = Instant::now();
            ctx.maybe_log_progress();

            let is_capture = move_captures_enemy(state, mv);
            let is_loud_promo = is_loud_promotion_move(state, mv);
            if is_capture {
                let mut hang_cache = LandingAttackCache::new();
                if capture_hangs_high_value_piece(state, weights, mv, true, &mut hang_cache) {
                    continue;
                }
            }
            let child_depth = d - 1;
            // Root LMR: late quiets at ID depth >= 2 (pre–PR17 rule).
            // Never reduce promotions into two-movers / range capturers.
            let can_reduce = d >= 2 && i >= 3 && !is_capture && !is_loud_promo && child_depth >= 1;
            let quiet_red = if can_reduce {
                (if i >= 12 { 2 } else { 1 }).min(child_depth)
            } else {
                0
            };
            let wipe_key = capturing_wipe_group_key(state, mv);

            let Some(undo) = pos.make_move_for_search(mv.clone()) else {
                continue;
            };
            ctx.last_ab_capture_enemy = move_loudness(state, weights, mv, is_capture);
            ctx.last_ab_to = Some(mv.to);
            ctx.last_ab_wipe = quiesce_move_looks_path_or_multileg(state, mv);
            ctx.last_ab_mover_large = mover_is_large(state, mv);
            ctx.nodes += 1;
            ctx.ply = root_ply + 1;
            ctx.phase = "search";
            ctx.q_label.clear();
            ctx.q_caps_at_node = 0;
            ctx.q_cap_index = 0;

            let e_i = -evaluate_with_ply(&pos, weights, ctx.ply);
            let landing_hit = pos
                .get_board()
                .is_position_attacked_by_color(mv.to, pos.get_current_turn());
            let sib = sibling_action(
                ctx.sibling_mode,
                wipe_key,
                &sib_reps,
                e_i,
                landing_hit,
                child_depth,
                i == 0,
            );
            let (sib_r, rel_expected, static_score) = match sib {
                SiblingAction::Full => (0, None, None),
                SiblingAction::Reduce { r, rel_expected } => (r, rel_expected, None),
                SiblingAction::Static { expected } => (0, None, Some(expected)),
            };
            let reduction = quiet_red.max(sib_r);
            if sib_r > 0 || static_score.is_some() {
                ctx.sib_reduced += 1;
            }

            // Root PVS: first move full window (PV); later moves null-window then
            // full-window research on fail-high (research keeps full q depth).
            let mut score = if let Some(expected) = static_score {
                expected
            } else if i == 0 {
                if reduction > 0 {
                    let reduced = child_depth - reduction;
                    -alphabeta(&mut pos, weights, reduced, -beta, -alpha, true, &mut ctx)
                } else {
                    -alphabeta(
                        &mut pos,
                        weights,
                        child_depth,
                        -beta,
                        -alpha,
                        true,
                        &mut ctx,
                    )
                }
            } else {
                ctx.root_pvs_tried += 1;
                let probe_a = rel_expected.map(|exp| alpha.max(exp)).unwrap_or(alpha);
                let nw_beta = probe_a.saturating_add(1);
                let mut s = if reduction > 0 {
                    let reduced = child_depth - reduction;
                    -alphabeta(
                        &mut pos, weights, reduced, -nw_beta, -probe_a, false, &mut ctx,
                    )
                } else {
                    -alphabeta(
                        &mut pos,
                        weights,
                        child_depth,
                        -nw_beta,
                        -probe_a,
                        false,
                        &mut ctx,
                    )
                };
                if !ctx.abort && s > probe_a {
                    ctx.root_fail_high += 1;
                    if sib_r > 0 {
                        ctx.sib_researched += 1;
                    }
                    s = -alphabeta(
                        &mut pos,
                        weights,
                        child_depth,
                        -beta,
                        -alpha,
                        true,
                        &mut ctx,
                    );
                } else if let Some(exp) = rel_expected {
                    if s <= probe_a && s >= exp.saturating_sub(50) {
                        s = exp;
                    }
                }
                s
            };
            if static_score.is_none() && i == 0 && reduction > 0 && !ctx.abort && score > alpha {
                if sib_r > 0 {
                    ctx.sib_researched += 1;
                }
                score = -alphabeta(
                    &mut pos,
                    weights,
                    child_depth,
                    -beta,
                    -alpha,
                    true,
                    &mut ctx,
                );
            }

            pos.unmake_move_for_search(undo);
            ctx.ply = root_ply;
            ctx.q_nodes_last_root = ctx.q_nodes.saturating_sub(ctx.q_nodes_at_root_start);

            if ctx.abort {
                finished_iteration = false;
                break;
            }
            if let Some(k) = wipe_key {
                sib_reps.entry(k).or_insert((score, e_i));
            }
            iter_lines.push((mv.clone(), score));
            ctx.root_moves_scored += 1;

            let improved = score > iter_score;
            if improved {
                iter_score = score;
                iter_best = mv.clone();
                ctx.best_score = iter_score;
                if !is_capture {
                    store_killer(&mut ctx, root_ply, move_tt_key(mv));
                    bump_history(&mut ctx, mv, d);
                }
            }
            if iter_score > i32::MIN + 1 && (score - iter_score).abs() < 20 {
                ctx.root_near_best += 1;
            }
            if score > alpha {
                alpha = score;
            }
            if score >= MATE_SCORE_BAND {
                break;
            }
        }

        if !finished_iteration {
            if completed_depth == 0 && !iter_lines.is_empty() {
                // Hard timeout: keep last completed iteration (partial d=1 only if nothing completed yet).
                iter_lines.sort_by(|a, b| b.1.cmp(&a.1));
                completed_lines = iter_lines;
                completed_best = iter_best;
                completed_score = iter_score;
                completed_depth = d;
            }
            break;
        }

        iter_lines.sort_by(|a, b| b.1.cmp(&a.1));
        completed_lines = iter_lines;
        completed_best = iter_best;
        completed_score = iter_score;
        completed_depth = d;
        ctx.best_score = completed_score;

        if d < max_depth {
            reorder_root_moves(&mut moves, &completed_best, &completed_lines);
        }
    }

    let best_move = completed_best;
    let best_score = completed_score;
    let root_lines = completed_lines;
    let depth_for_trace = completed_depth.max(1);

    // One-shot diagnosis summary for midgame dumps.
    {
        let fh_pct = if ctx.root_pvs_tried == 0 {
            0
        } else {
            (ctx.root_fail_high.saturating_mul(100)) / ctx.root_pvs_tried
        };
        let near_pct = if ctx.root_moves_scored == 0 {
            0
        } else {
            (ctx.root_near_best.saturating_mul(100)) / ctx.root_moves_scored
        };
        let mut scores: Vec<i32> = root_lines.iter().map(|(_, s)| *s).collect();
        scores.sort_unstable();
        let spread = if scores.is_empty() {
            0
        } else {
            scores[scores.len() - 1] - scores[scores.len() / 2]
        };
        let quniq = ctx.q_unique.len();
        let qhit_pct = if ctx.q_tt_probes == 0 {
            0
        } else {
            (ctx.q_tt_hits.saturating_mul(100)) / ctx.q_tt_probes
        };
        eprintln!(
            "ab diag: root={} scored={} fh={}% near20={}% spread(max-med)={} qnodes={} quniq={}{} qTThit={}%/{}/{} sib={}/{} abort={}",
            ctx.root_total,
            ctx.root_moves_scored,
            fh_pct,
            near_pct,
            spread,
            ctx.q_nodes,
            quniq,
            if ctx.q_unique_saturated { "+" } else { "" },
            qhit_pct,
            ctx.q_tt_hits,
            ctx.q_tt_probes,
            ctx.sib_reduced,
            ctx.sib_researched,
            ctx.abort
        );
    }

    let tree = if config.collect_trace {
        build_trace_tree(
            state,
            weights,
            depth_for_trace,
            root_ply,
            &root_lines,
            &best_move,
            best_score,
            static_eval,
            &mut ctx,
        )
    } else {
        SearchTreeNode {
            label: "root".into(),
            score: Some(best_score),
            static_eval: Some(static_eval),
            best: true,
            cutoff: false,
            children: vec![],
        }
    };

    SearchResult {
        best_move: Some(best_move),
        score: best_score,
        nodes: ctx.nodes,
        static_eval,
        root_lines,
        tree,
        q_nodes: ctx.q_nodes,
        q_caps_generated: ctx.q_caps_generated,
        q_caps_searched: ctx.q_caps_searched,
        q_tt_hits: ctx.q_tt_hits,
        q_tt_probes: ctx.q_tt_probes,
        q_kind_path: ctx.q_kind_path,
        q_kind_simple: ctx.q_kind_simple,
        q_kind_multi: ctx.q_kind_multi,
        q_unique: ctx.q_unique.len() as u64,
        q_unique_saturated: ctx.q_unique_saturated,
        root_moves_scored: ctx.root_moves_scored,
        aborted: ctx.abort,
    }
}

/// Leaf-only quiescence probe (open window) for prune measurement harnesses.
pub fn probe_quiescence(
    state: &GameState,
    weights: &EvalWeights,
    qdepth: u32,
    mode: QPruneMode,
    max_time_ms: Option<u64>,
) -> SearchResult {
    let _wbind = bind_search_weights(weights);
    let root_ply = state.get_move_history().len();
    let static_eval = evaluate_with_ply(state, weights, root_ply);
    let deadline = max_time_ms.map(|ms| Instant::now() + Duration::from_millis(ms));
    let now = Instant::now();
    let mut ctx = SearchContext {
        deadline,
        nodes: 0,
        abort: false,
        ply: root_ply,
        quiescence_depth: qdepth,
        quiesce_entry_depth: qdepth,
        search_started: now,
        last_progress_log: now,
        search_depth: 0,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "quiesce",
        tt: TranspositionTable::new(1024),
        q_tt: TranspositionTable::new(1 << 16),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy: 0.0,
        last_ab_to: None,
        last_ab_wipe: false,
        last_ab_mover_large: false,
        q_nodes: 0,
        q_nodes_at_root_start: 0,
        q_nodes_last_root: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: mode,
        q_caps_generated: 0,
        q_caps_searched: 0,
        q_kind_simple: 0,
        q_kind_path: 0,
        q_kind_multi: 0,
        root_pvs_tried: 0,
        root_fail_high: 0,
        root_near_best: 0,
        root_moves_scored: 0,
        q_unique: HashSet::new(),
        q_unique_saturated: false,
        q_tt_hits: 0,
        q_tt_probes: 0,
        root_move_started: Instant::now(),
        q_hash_prev_to: false,
        q_no_pathclear_reply: false,
        q_no_pathclear: false,
        q_loud_promo_simple_only: false,
        track_q_unique: false,
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        hang_q_dest_multileg: true,
        hang_q_dest_pathclear: true,
        q_open_large_mover: false,
        q_open_any_capture: false,
        q_recapture_only: false,
        q_own_large_only: false,
        sib_reduced: 0,
        sib_researched: 0,
    };
    let mut pos = state.clone();
    pos.ensure_eval_inc(weights);
    let score = if qdepth == 0 {
        static_eval
    } else {
        ctx.quiesce_entry_depth = qdepth;
        quiesce(
            &mut pos,
            weights,
            qdepth,
            i32::MIN + 1,
            i32::MAX - 1,
            None,
            !ctx.q_no_pathclear,
            true,
            &mut ctx,
        )
    };
    SearchResult {
        best_move: None,
        score,
        nodes: ctx.nodes,
        static_eval,
        root_lines: vec![],
        tree: SearchTreeNode {
            label: "qprobe".into(),
            score: Some(score),
            static_eval: Some(static_eval),
            best: true,
            cutoff: false,
            children: vec![],
        },
        q_nodes: ctx.q_nodes,
        q_caps_generated: ctx.q_caps_generated,
        q_caps_searched: ctx.q_caps_searched,
        q_tt_hits: ctx.q_tt_hits,
        q_tt_probes: ctx.q_tt_probes,
        q_kind_path: ctx.q_kind_path,
        q_kind_simple: ctx.q_kind_simple,
        q_kind_multi: ctx.q_kind_multi,
        q_unique: ctx.q_unique.len() as u64,
        q_unique_saturated: ctx.q_unique_saturated,
        root_moves_scored: 0,
        aborted: ctx.abort,
    }
}

/// Test harness: PathAware q with an explicit window and contested square.
#[cfg(test)]
fn probe_quiesce_window(
    state: &GameState,
    weights: &EvalWeights,
    qdepth: u32,
    alpha: i32,
    beta: i32,
    prev_to: Option<Position>,
) -> (i32, u64) {
    let _wbind = bind_search_weights(weights);
    let root_ply = state.get_move_history().len();
    let now = Instant::now();
    let mut ctx = SearchContext {
        deadline: None,
        nodes: 0,
        abort: false,
        ply: root_ply,
        quiescence_depth: qdepth,
        quiesce_entry_depth: qdepth,
        search_started: now,
        last_progress_log: now,
        search_depth: 0,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "quiesce",
        tt: TranspositionTable::new(1024),
        q_tt: TranspositionTable::new(1 << 16),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy: min_quiescence_enemy_material(),
        last_ab_to: prev_to,
        last_ab_wipe: false,
        last_ab_mover_large: false,
        q_nodes: 0,
        q_nodes_at_root_start: 0,
        q_nodes_last_root: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: QPruneMode::PathAware,
        q_caps_generated: 0,
        q_caps_searched: 0,
        q_kind_simple: 0,
        q_kind_path: 0,
        q_kind_multi: 0,
        root_pvs_tried: 0,
        root_fail_high: 0,
        root_near_best: 0,
        root_moves_scored: 0,
        q_unique: HashSet::new(),
        q_unique_saturated: false,
        q_tt_hits: 0,
        q_tt_probes: 0,
        root_move_started: Instant::now(),
        q_hash_prev_to: false,
        q_no_pathclear_reply: false,
        q_no_pathclear: false,
        q_loud_promo_simple_only: false,
        track_q_unique: false,
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        hang_q_dest_multileg: true,
        hang_q_dest_pathclear: true,
        q_open_large_mover: false,
        q_open_any_capture: false,
        q_recapture_only: false,
        q_own_large_only: false,
        sib_reduced: 0,
        sib_researched: 0,
    };
    let mut pos = state.clone();
    pos.ensure_eval_inc(weights);
    let score = quiesce(
        &mut pos, weights, qdepth, alpha, beta, prev_to, true, true, &mut ctx,
    );
    (score, ctx.q_nodes)
}

/// Test harness: quiet-parent leaf path (`last_ab_capture_enemy = 0`).
#[cfg(test)]
fn probe_quiet_parent_leaf_or_quiesce(
    state: &GameState,
    weights: &EvalWeights,
    qdepth: u32,
) -> (i32, u64) {
    probe_quiet_parent_leaf_or_quiesce_hang(state, weights, qdepth, QHangOpts::default())
}

#[cfg(test)]
fn probe_quiet_parent_leaf_or_quiesce_hang(
    state: &GameState,
    weights: &EvalWeights,
    qdepth: u32,
    hang: QHangOpts,
) -> (i32, u64) {
    let _wbind = bind_search_weights(weights);
    let root_ply = state.get_move_history().len();
    let now = Instant::now();
    let mut ctx = SearchContext {
        deadline: None,
        nodes: 0,
        abort: false,
        ply: root_ply,
        quiescence_depth: qdepth,
        quiesce_entry_depth: qdepth,
        search_started: now,
        last_progress_log: now,
        search_depth: 0,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "leaf",
        tt: TranspositionTable::new(1024),
        q_tt: TranspositionTable::new(1 << 16),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy: 0.0,
        last_ab_to: None,
        last_ab_wipe: false,
        last_ab_mover_large: false,
        q_nodes: 0,
        q_nodes_at_root_start: 0,
        q_nodes_last_root: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: QPruneMode::PathAware,
        q_caps_generated: 0,
        q_caps_searched: 0,
        q_kind_simple: 0,
        q_kind_path: 0,
        q_kind_multi: 0,
        root_pvs_tried: 0,
        root_fail_high: 0,
        root_near_best: 0,
        root_moves_scored: 0,
        q_unique: HashSet::new(),
        q_unique_saturated: false,
        q_tt_hits: 0,
        q_tt_probes: 0,
        root_move_started: Instant::now(),
        q_hash_prev_to: false,
        q_no_pathclear_reply: false,
        q_no_pathclear: false,
        q_loud_promo_simple_only: false,
        track_q_unique: false,
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        hang_q_dest_multileg: hang.dest_multileg,
        hang_q_dest_pathclear: hang.dest_pathclear,
        q_open_large_mover: false,
        q_open_any_capture: false,
        q_recapture_only: false,
        q_own_large_only: false,
        sib_reduced: 0,
        sib_researched: 0,
    };
    let mut pos = state.clone();
    pos.ensure_eval_inc(weights);
    let score = leaf_or_quiesce(
        &mut pos,
        weights,
        i32::MIN + 1,
        i32::MAX - 1,
        true,
        &mut ctx,
    );
    (score, ctx.q_nodes)
}

/// Capture-parent leaf with optional R/S1/S2 flags (`last_ab_capture_enemy` set).
#[cfg(test)]
fn probe_capture_parent_leaf_or_quiesce_rs(
    state: &GameState,
    weights: &EvalWeights,
    qdepth: u32,
    last_ab_capture_enemy: f32,
    last_ab_to: Option<Position>,
    last_ab_mover_large: bool,
    q_open_large_mover: bool,
    q_open_any_capture: bool,
    q_recapture_only: bool,
    q_own_large_only: bool,
) -> (i32, u64) {
    let _wbind = bind_search_weights(weights);
    let root_ply = state.get_move_history().len();
    let now = Instant::now();
    let mut ctx = SearchContext {
        deadline: None,
        nodes: 0,
        abort: false,
        ply: root_ply,
        quiescence_depth: qdepth,
        quiesce_entry_depth: qdepth,
        search_started: now,
        last_progress_log: now,
        search_depth: 0,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "leaf",
        tt: TranspositionTable::new(1024),
        q_tt: TranspositionTable::new(1 << 16),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy,
        last_ab_to,
        last_ab_wipe: false,
        last_ab_mover_large,
        q_nodes: 0,
        q_nodes_at_root_start: 0,
        q_nodes_last_root: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: QPruneMode::PathAware,
        q_caps_generated: 0,
        q_caps_searched: 0,
        q_kind_simple: 0,
        q_kind_path: 0,
        q_kind_multi: 0,
        root_pvs_tried: 0,
        root_fail_high: 0,
        root_near_best: 0,
        root_moves_scored: 0,
        q_unique: HashSet::new(),
        q_unique_saturated: false,
        q_tt_hits: 0,
        q_tt_probes: 0,
        root_move_started: Instant::now(),
        q_hash_prev_to: false,
        q_no_pathclear_reply: false,
        q_no_pathclear: false,
        q_loud_promo_simple_only: false,
        track_q_unique: false,
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        hang_q_dest_multileg: false,
        hang_q_dest_pathclear: false,
        q_open_large_mover,
        q_open_any_capture,
        q_recapture_only,
        q_own_large_only,
        sib_reduced: 0,
        sib_researched: 0,
    };
    let mut pos = state.clone();
    pos.ensure_eval_inc(weights);
    let score = leaf_or_quiesce(
        &mut pos,
        weights,
        i32::MIN + 1,
        i32::MAX - 1,
        true,
        &mut ctx,
    );
    (score, ctx.q_nodes)
}

fn same_root_move(a: &Move, b: &Move) -> bool {
    a.from == b.from && a.to == b.to && a.promoted == b.promoted
}

/// After an ID iteration: previous best first, then by that iteration's scores, else unchanged.
fn reorder_root_moves(moves: &mut Vec<Move>, best: &Move, scored: &[(Move, i32)]) {
    moves.sort_by(|a, b| {
        let a_best = same_root_move(a, best);
        let b_best = same_root_move(b, best);
        match (a_best, b_best) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => {
                let sa = scored
                    .iter()
                    .find(|(m, _)| same_root_move(m, a))
                    .map(|(_, s)| *s)
                    .unwrap_or(i32::MIN);
                let sb = scored
                    .iter()
                    .find(|(m, _)| same_root_move(m, b))
                    .map(|(_, s)| *s)
                    .unwrap_or(i32::MIN);
                sb.cmp(&sa)
            }
        }
    });
}

/// After the main search, build a capped GUI tree without changing the chosen move.
fn build_trace_tree(
    state: &GameState,
    weights: &EvalWeights,
    depth: u32,
    root_ply: usize,
    root_lines: &[(Move, i32)],
    best_move: &Move,
    best_score: i32,
    static_eval: i32,
    ctx: &mut SearchContext,
) -> SearchTreeNode {
    let mut children: Vec<SearchTreeNode> = root_lines
        .iter()
        .take(MAX_TREE_ROOT_CHILDREN)
        .map(|(mv, score)| {
            let is_best = mv.from == best_move.from
                && mv.to == best_move.to
                && mv.promoted == best_move.promoted;
            SearchTreeNode {
                label: move_label(state, mv),
                score: Some(*score),
                static_eval: None,
                best: is_best,
                cutoff: false,
                children: vec![],
            }
        })
        .collect();

    // Expand replies only under the best root move (one extra depth-1 search with recording).
    if depth > 1 {
        if let Some(best_node) = children.iter_mut().find(|c| c.best) {
            let mut child = state.clone();
            if let Some(undo) = child.make_move_for_search(best_move.clone()) {
                ctx.last_ab_capture_enemy = move_loudness(
                    state,
                    weights,
                    best_move,
                    move_captures_enemy(state, best_move),
                );
                ctx.last_ab_to = Some(best_move.to);
                ctx.last_ab_wipe = quiesce_move_looks_path_or_multileg(state, best_move);
                ctx.last_ab_mover_large = mover_is_large(state, best_move);
                ctx.ply = root_ply + 1;
                ctx.phase = "trace";
                let (_score, subtree) = alphabeta_record(&mut child, weights, depth - 1, &mut *ctx);
                child.unmake_move_for_search(undo);
                if let Some(sub) = subtree {
                    best_node.children = sub.children;
                }
            }
        }
    }

    SearchTreeNode {
        label: "root".into(),
        score: Some(best_score),
        static_eval: Some(static_eval),
        best: true,
        cutoff: false,
        children,
    }
}

fn alphabeta(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    is_pv: bool,
    ctx: &mut SearchContext,
) -> i32 {
    ctx.nodes += 1;

    if cfg!(debug_assertions) && ctx.nodes % 256 == 0 {
        debug_assert_eq!(
            state.hash(),
            crate::zobrist::compute(state),
            "zobrist drift at node {}",
            ctx.nodes
        );
    }

    if state.get_winner().is_some() || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }
    // Revisit on the path → draw (do not probe/store TT).
    if state.is_repetition_draw_for_search() {
        return 0;
    }
    if state.is_draw_by_progress_rule() {
        return 0;
    }
    if depth == 0 {
        return leaf_or_quiesce(state, weights, alpha, beta, is_pv, ctx);
    }

    let key = position_hash(state);
    let alpha_orig = alpha;
    let mut tt_move: Option<MoveKey> = None;
    if let Some(e) = ctx.tt.probe(key) {
        if e.depth >= depth {
            match e.bound {
                TtBound::Exact => return e.score,
                TtBound::Lower => {
                    if e.score >= beta {
                        return e.score;
                    }
                    alpha = alpha.max(e.score);
                }
                TtBound::Upper => {
                    if e.score <= alpha {
                        return e.score;
                    }
                }
            }
        }
        tt_move = e.best;
    }

    // Null-move prune: pass and search shallow; if still >= beta, cutoff.
    // Ultimate Shogi almost always has near-null quiets, so zugzwang is rare.
    // Note: root calls alphabeta with depth = ID_depth - 1, so thresholds must
    // fire at depth >= 2 to help opening d3 searches.
    // Null leaf (depth 0 after R) uses stand-pat eval — not quiescence.
    const NULL_R: u32 = 2;
    if ctx.allow_null && depth >= 2 && beta < MATE_SCORE_BAND && beta > -MATE_SCORE_BAND {
        let r = NULL_R.min(depth - 1);
        ctx.allow_null = false;
        let prev_turn = state.get_current_turn();
        let saved_enemy = ctx.last_ab_capture_enemy;
        let saved_to = ctx.last_ab_to;
        let saved_wipe = ctx.last_ab_wipe;
        let saved_mover_large = ctx.last_ab_mover_large;
        ctx.last_ab_capture_enemy = 0.0;
        ctx.last_ab_to = None;
        ctx.last_ab_wipe = false;
        ctx.last_ab_mover_large = false;
        state.set_current_turn(prev_turn.opposite());
        state.push_repetition_key();
        let parent_ply = ctx.ply;
        ctx.ply = parent_ply + 1;
        let null_depth = depth - 1 - r;
        let score = if null_depth == 0 {
            -evaluate_with_ply(state, weights, ctx.ply)
        } else {
            -alphabeta(state, weights, null_depth, -beta, -beta + 1, false, ctx)
        };
        ctx.ply = parent_ply;
        state.pop_repetition_key();
        state.set_current_turn(prev_turn);
        ctx.last_ab_capture_enemy = saved_enemy;
        ctx.last_ab_to = saved_to;
        ctx.last_ab_wipe = saved_wipe;
        ctx.last_ab_mover_large = saved_mover_large;
        ctx.allow_null = true;
        if !ctx.abort && score >= beta {
            return score;
        }
    }

    // Stage A: captures + non-multi-leg quiets (+ capturing multi-leg).
    // Stage B: quiet two-step / FreeEagle only if A did not cut.
    let mut moves = state.generate_legal_moves_mode(LegalMoveGen::WithoutQuietMultiLeg);
    let mut used_only_stage_b = false;
    if moves.is_empty() {
        moves = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);
        used_only_stage_b = true;
        if moves.is_empty() {
            return evaluate_with_ply(state, weights, ctx.ply);
        }
    }
    if used_only_stage_b {
        order_moves_with_heuristics(state, weights, &mut moves, ctx, ctx.ply, true, false);
    } else {
        order_moves_with_heuristics(state, weights, &mut moves, ctx, ctx.ply, false, false);
    }
    prefer_tt_move(&mut moves, tt_move);

    let parent_ply = ctx.ply;
    let stage_a_len = moves.len();
    let (mut best, mut best_move_key, mut alpha, did_cutoff) = search_move_list(
        state, weights, depth, alpha, beta, is_pv, ctx, parent_ply, &moves, 0,
    );

    if !did_cutoff && !ctx.abort && !used_only_stage_b {
        let mut stage_b = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);
        if !stage_b.is_empty() {
            order_moves_with_heuristics(state, weights, &mut stage_b, ctx, parent_ply, true, false);
            prefer_tt_move(&mut stage_b, tt_move);
            let (b2, k2, a2, _cut2) = search_move_list(
                state,
                weights,
                depth,
                alpha,
                beta,
                is_pv,
                ctx,
                parent_ply,
                &stage_b,
                stage_a_len,
            );
            if b2 > best {
                best = b2;
                best_move_key = k2;
            }
            alpha = a2;
        }
    }

    if best == i32::MIN + 1 {
        return evaluate_with_ply(state, weights, ctx.ply);
    }

    if !ctx.abort {
        let bound = if best <= alpha_orig {
            TtBound::Upper
        } else if best >= beta {
            TtBound::Lower
        } else {
            TtBound::Exact
        };
        ctx.tt.store(TtEntry {
            key,
            depth,
            score: best,
            bound,
            best: best_move_key,
        });
    }

    best
}

fn prefer_tt_move(moves: &mut [Move], tt_move: Option<MoveKey>) {
    if let Some(tm) = tt_move {
        if let Some(idx) = moves.iter().position(|m| same_tt_move(m, tm)) {
            moves.swap(0, idx);
        }
    }
}

fn search_move_list(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    is_pv: bool,
    ctx: &mut SearchContext,
    parent_ply: usize,
    moves: &[Move],
    move_index_base: usize,
) -> (i32, Option<MoveKey>, i32, bool) {
    let mut best = i32::MIN + 1;
    let mut best_move_key = None;
    let mut did_cutoff = false;
    let mut hang_cache = LandingAttackCache::new();
    let mut sib_reps: HashMap<u64, (i32, i32)> = HashMap::new();

    for (i, mv) in moves.iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        let mv_key = move_tt_key(&mv);
        let is_capture = move_captures_enemy(state, &mv);
        let is_loud_promo = is_loud_promotion_move(state, &mv);
        if is_capture && capture_hangs_high_value_piece(state, weights, &mv, false, &mut hang_cache)
        {
            continue;
        }
        let is_killer = killer_rank(ctx, parent_ply, &mv) > 0;
        let move_index = move_index_base + i;

        // LMR: late quiet non-killers get a reduced child search.
        // depth is the remaining AB depth at this node (root passes ID_depth-1).
        const LMR_MIN_DEPTH: u32 = 2;
        const LMR_MOVE_THRESHOLD: usize = 3;
        const LMR_R: u32 = 1;
        let can_reduce = depth >= LMR_MIN_DEPTH
            && move_index >= LMR_MOVE_THRESHOLD
            && !is_capture
            && !is_loud_promo
            && !is_killer;
        let quiet_red = if can_reduce {
            if move_index >= 12 { 2 } else { LMR_R }.min(depth - 1)
        } else {
            0
        };
        let wipe_key = capturing_wipe_group_key(state, &mv);
        let is_wipe = quiesce_move_looks_path_or_multileg(state, &mv);
        let child_depth = depth.saturating_sub(1);

        let capture_enemy = move_loudness(state, weights, &mv, is_capture);
        let mover_large = mover_is_large(state, &mv);
        let Some(undo) = state.make_move_for_search(mv.clone()) else {
            continue;
        };
        ctx.last_ab_capture_enemy = capture_enemy;
        ctx.last_ab_to = Some(mv.to);
        ctx.last_ab_wipe = is_wipe;
        ctx.last_ab_mover_large = mover_large;
        ctx.ply = parent_ply + 1;

        let e_i = -evaluate_with_ply(state, weights, ctx.ply);
        let landing_hit = state
            .get_board()
            .is_position_attacked_by_color(mv.to, state.get_current_turn());
        let sib = sibling_action(
            ctx.sibling_mode,
            wipe_key,
            &sib_reps,
            e_i,
            landing_hit,
            child_depth,
            move_index == 0,
        );
        let (sib_r, rel_expected, static_score) = match sib {
            SiblingAction::Full => (0, None, None),
            SiblingAction::Reduce { r, rel_expected } => (r, rel_expected, None),
            SiblingAction::Static { expected } => (0, None, Some(expected)),
        };
        let reduction = quiet_red.max(sib_r);
        if sib_r > 0 || static_score.is_some() {
            ctx.sib_reduced += 1;
        }

        // PV only along the first move of a PV node (root PVS / research sets
        // is_pv). Non-PV leaves use capped quiescence when config > 2.
        let child_pv = is_pv && i == 0 && reduction == 0;

        let mut score = if let Some(expected) = static_score {
            expected
        } else if let Some(exp) = rel_expected {
            let probe_a = alpha.max(exp);
            let reduced = child_depth - reduction;
            let s = -alphabeta(
                state,
                weights,
                reduced,
                -(probe_a.saturating_add(1)),
                -probe_a,
                false,
                ctx,
            );
            if !ctx.abort && s > probe_a {
                ctx.sib_researched += 1;
                -alphabeta(
                    state,
                    weights,
                    child_depth,
                    -beta,
                    -alpha,
                    is_pv && i == 0,
                    ctx,
                )
            } else if s <= probe_a && s >= exp.saturating_sub(50) {
                exp
            } else {
                s
            }
        } else if reduction > 0 {
            let reduced = depth - 1 - reduction;
            -alphabeta(state, weights, reduced, -beta, -alpha, false, ctx)
        } else {
            -alphabeta(state, weights, depth - 1, -beta, -alpha, child_pv, ctx)
        };

        // Re-search at full depth if reduced search looks interesting.
        // Fail-high research restores PV (full q) when the parent was PV.
        if static_score.is_none()
            && rel_expected.is_none()
            && reduction > 0
            && !ctx.abort
            && score > alpha
        {
            if sib_r > 0 {
                ctx.sib_researched += 1;
            }
            score = -alphabeta(
                state,
                weights,
                depth - 1,
                -beta,
                -alpha,
                is_pv && i == 0,
                ctx,
            );
        }

        state.unmake_move_for_search(undo);
        ctx.ply = parent_ply;
        if !ctx.abort {
            if let Some(k) = wipe_key {
                sib_reps.entry(k).or_insert((score, e_i));
            }
        }

        if score > best {
            best = score;
            best_move_key = Some(mv_key);
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            did_cutoff = true;
            if !is_capture {
                store_killer(ctx, parent_ply, mv_key);
                bump_history(ctx, &mv, depth);
            }
            break;
        }
    }
    (best, best_move_key, alpha, did_cutoff)
}

/// Quiescence ply budget for this leaf: full config on PV / fail-high research;
/// null-window (non-PV) caps at 1 to keep deep ID (d3/d4) interactive.
fn leaf_quiescence_depth(ctx: &SearchContext, is_pv: bool) -> u32 {
    let cfg = ctx.quiescence_depth;
    if cfg == 0 || is_pv {
        cfg
    } else {
        cfg.min(1)
    }
}

fn leaf_or_quiesce(
    state: &mut GameState,
    weights: &EvalWeights,
    alpha: i32,
    beta: i32,
    is_pv: bool,
    ctx: &mut SearchContext,
) -> i32 {
    // Q after loud AB captures/promos, loud promos, a free take of a hanging
    // large enemy, or any King/CP capture (two-step dest included — one ply).
    // Optional R/S1/S2 flags open q after sub-loud captures or own-large dest takes.
    let loud_parent = ctx.last_ab_capture_enemy >= min_quiescence_enemy_material();
    let loud_promos = generate_loud_promotions(state);
    let hang_opts = QHangOpts::from_ctx(ctx);
    let hang_caps = !loud_parent && stm_has_large_hang_take(state, weights, hang_opts);
    let royal_caps = !loud_parent && stm_has_royal_capture(state);
    let capture_parent = ctx.last_ab_capture_enemy > 0.0;
    let large_mover_open = ctx.q_open_large_mover && ctx.last_ab_mover_large && capture_parent;
    let any_cap_open = ctx.q_open_any_capture && capture_parent;
    let own_large_open =
        ctx.q_own_large_only && stm_has_dest_take_of_prev_large(state, ctx.last_ab_to);
    let include_caps = loud_parent
        || hang_caps
        || royal_caps
        || large_mover_open
        || any_cap_open
        || own_large_open;
    if !include_caps && loud_promos.is_empty() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }
    let q = leaf_quiescence_depth(ctx, is_pv);
    if q == 0 {
        evaluate_with_ply(state, weights, ctx.ply)
    } else {
        ctx.phase = "quiesce";
        ctx.quiesce_entry_depth = q;
        // Prefer AB landing (search skips move_history); history as fallback.
        let prev_to = ctx
            .last_ab_to
            .or_else(|| state.get_move_history().last().map(|m| m.to));
        let after_wipe = ctx.q_no_pathclear_after_wipe && ctx.last_ab_wipe;
        // Quiet leaf with only promo tactics: don't open full capture q.
        // Hang-caps open capture q so free large SimpleTakes get resolved.
        quiesce(
            state,
            weights,
            q,
            alpha,
            beta,
            prev_to,
            !ctx.q_no_pathclear && !after_wipe,
            include_caps,
            ctx,
        )
    }
}

/// Capture-only quiescence (excludes pure self-captures via `move_captures_enemy`),
/// plus promotions into two-movers / range capturers.
///
/// Q contract: resolve hanging exchanges on the contested square (SimpleTakes /
/// dest-recaptures). Non-recapture PathClear/MultiLeg corridor tactics are left
/// to main-search depth (pre–PR 17-style behavior under high piece values).
/// Loud promotions (FreeKing→GG, etc.) are always eligible and skip PathAware
/// top-N / delta / hang cuts.
///
/// `prev_to`: prior move landing (PathAware recapture / RecaptureOnly).
/// `allow_pathclear`: dest-recapture PathClear/MultiLeg are eligible. D1 clears
/// this after a corridor wipe so the reply cannot be another file rewrite.
/// `include_captures`: false = promo-only entry after a quiet AB leaf without
/// hanging large SimpleTakes; true for loud parents or hang-cap quiet leaves.
fn quiesce(
    state: &mut GameState,
    weights: &EvalWeights,
    qdepth: u32,
    mut alpha: i32,
    beta: i32,
    prev_to: Option<Position>,
    allow_pathclear: bool,
    include_captures: bool,
    ctx: &mut SearchContext,
) -> i32 {
    ctx.nodes += 1;
    ctx.q_nodes += 1;
    ctx.q_depth_left = qdepth;

    if state.get_winner().is_some() || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }
    if state.is_repetition_draw_for_search() {
        return 0;
    }
    if state.is_draw_by_progress_rule() {
        return 0;
    }

    let key = q_tt_key(state, prev_to, ctx.q_hash_prev_to);
    // Unique-q tracking is diagnostic-only unless `track_q_unique`.
    if ctx.track_q_unique || cfg!(debug_assertions) {
        if !ctx.q_unique_saturated {
            if ctx.q_unique.len()
                < if ctx.track_q_unique {
                    1 << 20
                } else {
                    Q_UNIQUE_CAP
                }
            {
                ctx.q_unique.insert(key);
            } else {
                ctx.q_unique_saturated = true;
            }
        }
    }

    // Quiescence TT: depth is remaining q-plies.
    ctx.q_tt_probes += 1;
    let mut tt_move: Option<MoveKey> = None;
    if let Some(e) = ctx.q_tt.probe(key) {
        if e.depth >= qdepth {
            ctx.q_tt_hits += 1;
            match e.bound {
                TtBound::Exact => return e.score,
                TtBound::Lower => {
                    if e.score >= beta {
                        return e.score;
                    }
                    alpha = alpha.max(e.score);
                }
                TtBound::Upper => {
                    if e.score <= alpha {
                        return e.score;
                    }
                }
            }
        }
        tt_move = e.best;
    }
    let alpha_orig = alpha;

    let stand_pat = evaluate_with_ply(state, weights, ctx.ply);
    ctx.q_stand_pat = stand_pat;
    if qdepth == 0 {
        return stand_pat;
    }
    let resolve_major = prev_to_is_major_enemy(state, weights, prev_to);
    let has_royal_take = stm_has_royal_capture(state);
    if stand_pat >= beta && !resolve_major && !has_royal_take {
        ctx.q_tt.store(TtEntry {
            key,
            depth: qdepth,
            score: stand_pat,
            bound: TtBound::Lower,
            best: None,
        });
        return stand_pat;
    }
    // Leave the window intact when deferring a stand-pat cutoff so dest
    // recaptures still expand; `best` starts at stand-pat.
    if stand_pat > alpha && stand_pat < beta {
        alpha = stand_pat;
    }

    let path_aware = ctx.q_prune_mode.uses_path_aware();
    let deep_ply = qdepth < ctx.quiesce_entry_depth;
    // Deep PathAware plies only need recaptures onto prev_to — skip full-board gen.
    // Child plies always allow captures; promo-only is entry-only.
    // S1 recapture-only also generates dest hits on prev_to from the first ply.
    let victim_only = (path_aware && deep_ply && prev_to.is_some())
        || (ctx.q_recapture_only && prev_to.is_some());
    let gen_captures = include_captures || deep_ply;
    let loud_st =
        ctx.q_loud_promo_simple_only || (ctx.q_no_pathclear_after_wipe && ctx.last_ab_wipe);
    let raw_moves = generate_quiescence_captures_with_hang(
        state,
        weights,
        prev_to,
        victim_only,
        gen_captures,
        allow_pathclear,
        loud_st,
        QHangOpts::from_ctx(ctx),
    );
    if raw_moves.is_empty() {
        return stand_pat;
    }

    struct QCand {
        mv: Move,
        enemy: f32,
        own: f32,
        kind: CaptureKind,
        mover_value: f32,
        is_recapture: bool,
        is_dest_recapture: bool,
        is_loud_promo: bool,
        /// Capture victim + promo material jump (for delta / ordering).
        tactical_gain: f32,
        /// Idea A/B dest-hang (keep even when not a dest recapture).
        is_hang_dest: bool,
        /// King / CP capture (two-step dest is one ply). Always keep.
        is_royal_take: bool,
    }

    let mut cands: Vec<QCand> = raw_moves
        .into_iter()
        .filter_map(|mv| {
            let mover_value = state
                .get_board()
                .get_piece(mv.from)
                .map(|p| material_piece_value(&p, weights))
                .unwrap_or(0.0);
            let is_recapture = prev_to
                .map(|sq| capture_hits_square(state, &mv, sq))
                .unwrap_or(false);
            let is_dest_recapture = prev_to == Some(mv.to);
            let (enemy, own, kind) = capture_exchange_kind(state, weights, &mv);
            let is_loud_promo = is_loud_promotion_move(state, &mv)
                && !(loud_st && matches!(kind, CaptureKind::PathClear | CaptureKind::MultiLeg));
            let promo_gain = if is_loud_promo {
                loud_promotion_material_gain(state, weights, &mv)
            } else {
                0.0
            };
            let is_hang_dest = dest_hang_kind(state, weights, &mv, QHangOpts::from_ctx(ctx))
                .is_some_and(|k| !matches!(k, CaptureKind::SimpleTake));
            let is_royal_take = capture_takes_enemy_royal(state, &mv);
            Some(QCand {
                mv,
                enemy,
                own,
                kind,
                mover_value,
                is_recapture,
                is_dest_recapture,
                is_loud_promo,
                tactical_gain: enemy + promo_gain,
                is_hang_dest,
                is_royal_take,
            })
        })
        .collect();

    // Recapture-only after the first q-ply.
    if ctx.q_prune_mode.uses_recapture_only() {
        if prev_to.is_some() {
            cands.retain(|c| c.is_recapture || c.is_loud_promo || c.is_royal_take);
            if cands.is_empty() {
                return stand_pat;
            }
        }
    }
    // S1: dest recapture onto prev_to only (plus royals / loud promos).
    if ctx.q_recapture_only && prev_to.is_some() {
        cands.retain(|c| c.is_dest_recapture || c.is_loud_promo || c.is_royal_take);
        if cands.is_empty() {
            return stand_pat;
        }
    }
    // S2: only dest takes of large enemies (plus royals / loud promos).
    if ctx.q_own_large_only {
        cands.retain(|c| c.is_loud_promo || c.is_royal_take || dest_victim_is_big(state, &c.mv));
        if cands.is_empty() {
            return stand_pat;
        }
    }

    // PathAware deep taper: loud victims, or recapture onto the previous landing.
    if path_aware && deep_ply {
        let floor = min_quiescence_deep_enemy();
        cands.retain(|c| c.enemy >= floor || c.is_recapture || c.is_loud_promo || c.is_royal_take);
        if cands.is_empty() {
            return stand_pat;
        }
    }

    let use_net = ctx.q_prune_mode.uses_net_gain();

    // Stale hang prune (pre-move landing attack).
    if ctx.q_prune_mode.uses_stale_hang() {
        let opponent = state.get_current_turn().opposite();
        let mut attack_cache = LandingAttackCache::new();
        let board = state.get_board();
        cands.retain(|c| {
            if c.is_loud_promo || capture_takes_enemy_royal(state, &c.mv) {
                return true;
            }
            if !net_below_hang_frac(c.enemy, c.own, c.mover_value) {
                return true;
            }
            !landing_attacked_cached(board, c.mv.to, opponent, &mut attack_cache)
        });
        if cands.is_empty() {
            return stand_pat;
        }
    }

    // Capturable-max futility: best legal candidate gain, not any piece on the board.
    let best_gain = cands
        .iter()
        .map(|c| {
            if c.is_loud_promo {
                c.tactical_gain
            } else if use_net {
                c.enemy - c.own
            } else {
                c.enemy
            }
        })
        .fold(0.0f32, f32::max);
    if !cands.iter().any(|c| c.is_royal_take)
        && stand_pat.saturating_add(best_gain.round() as i32) <= alpha
    {
        return stand_pat;
    }

    // Last-royal (instant win), then loud promo, path-sum, dest recapture, net MVV-LVA.
    cands.sort_by(|a, b| {
        let win_a = capture_takes_last_enemy_royal(state, &a.mv);
        let win_b = capture_takes_last_enemy_royal(state, &b.mv);
        let win = win_a.cmp(&win_b);
        if win != std::cmp::Ordering::Equal {
            return win.reverse();
        }
        let promo_a = a.is_loud_promo.cmp(&b.is_loud_promo);
        if promo_a != std::cmp::Ordering::Equal {
            return promo_a.reverse();
        }
        let ga = (a.tactical_gain * 1000.0).round() as i32;
        let gb = (b.tactical_gain * 1000.0).round() as i32;
        gb.cmp(&ga)
            .then_with(|| b.is_dest_recapture.cmp(&a.is_dest_recapture))
            .then_with(|| b.is_recapture.cmp(&a.is_recapture))
            .then_with(|| {
                let sa = ((a.tactical_gain - a.own) * 1000.0 - a.mover_value).round() as i32;
                let sb = ((b.tactical_gain - b.own) * 1000.0 - b.mover_value).round() as i32;
                sb.cmp(&sa)
            })
    });
    if let Some(tm) = tt_move {
        if let Some(idx) = cands.iter().position(|c| same_tt_move(&c.mv, tm)) {
            cands.swap(0, idx);
        }
    }

    ctx.q_caps_generated = ctx.q_caps_generated.saturating_add(cands.len() as u64);

    let top_n_cap = if ctx.q_prune_mode.uses_top_n() {
        if path_aware {
            if deep_ply {
                QUIESCE_TOP_N_DEEP
            } else {
                QUIESCE_TOP_N_PATH_AWARE_ROOT
            }
        } else {
            QUIESCE_TOP_N
        }
    } else {
        usize::MAX
    };

    // PathAware: PathClear/MultiLeg under budget only when they answer a capture
    // or land on a loud piece (drop mop PathClears whose value is path-sum junk).
    // Deep plies also restrict SimpleTakes to recaptures onto the previous landing.
    // Loud promotions are always kept (outside the top-N budget).
    if path_aware {
        let mut kept = Vec::with_capacity(top_n_cap.min(cands.len()) + 4);
        let mut path_kept = 0usize;
        let mut non_promo = 0usize;
        for c in cands.drain(..) {
            if c.is_loud_promo || c.is_royal_take || (resolve_major && c.is_dest_recapture) {
                kept.push(c);
                continue;
            }
            if non_promo >= top_n_cap {
                continue;
            }
            match c.kind {
                CaptureKind::SimpleTake => {
                    if deep_ply && !c.is_recapture {
                        continue;
                    }
                }
                CaptureKind::PathClear | CaptureKind::MultiLeg => {
                    if !allow_pathclear {
                        continue;
                    }
                    if path_kept >= QUIESCE_PATHCLEAR_DEEP_BUDGET {
                        continue;
                    }
                    if !pathclear_allowed_in_pathaware_q(c.is_dest_recapture) && !c.is_hang_dest {
                        continue;
                    }
                    path_kept += 1;
                }
            }
            non_promo += 1;
            kept.push(c);
        }
        cands = kept;
    } else if top_n_cap != usize::MAX && cands.len() > top_n_cap {
        // Keep all loud promos, truncate the rest.
        let mut promos = Vec::new();
        let mut rest = Vec::new();
        for c in cands.drain(..) {
            if c.is_loud_promo {
                promos.push(c);
            } else if rest.len() < top_n_cap {
                rest.push(c);
            }
        }
        promos.append(&mut rest);
        cands = promos;
    }
    if let Some(tm) = tt_move {
        if let Some(idx) = cands.iter().position(|c| same_tt_move(&c.mv, tm)) {
            cands.swap(0, idx);
        }
    }

    let n_caps = cands.len();
    ctx.q_caps_at_node = n_caps;

    let mut best = stand_pat;
    let mut best_move_key: Option<MoveKey> = None;
    let parent_ply = ctx.ply;
    let opponent = state.get_current_turn().opposite();

    for (i, c) in cands.into_iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        // Live delta: skip once earlier MVV takes have raised alpha.
        // Loud promos always expand (promo gain is the point of searching them).
        let gain = if c.is_loud_promo {
            c.tactical_gain
        } else if use_net {
            c.enemy - c.own
        } else {
            c.enemy
        };
        if !c.is_loud_promo
            && !c.is_royal_take
            && !(resolve_major && c.is_dest_recapture)
            && (stand_pat as f32 + gain) <= alpha as f32
        {
            continue;
        }
        ctx.q_cap_index = i + 1;
        ctx.q_label = move_label(state, &c.mv);
        ctx.phase = "quiesce";
        // Periodic progress while a single loud capture line is exploding.
        if ctx.q_nodes & 0xff == 0 {
            ctx.maybe_log_progress();
        }

        let kind = c.kind;
        let enemy = c.enemy;
        let own = c.own;
        let mover_value = c.mover_value;
        let landing = c.mv.to;
        let mv_key = move_tt_key(&c.mv);
        let mv = c.mv;
        let is_loud_promo = c.is_loud_promo;
        let takes_royal = capture_takes_enemy_royal(state, &mv);

        // Pre-make hang skip for SimpleTake only. PathClear/MultiLeg often look
        // attacked pre-move only because a path victim "defends" the landing; those
        // go through make + post-fire check below.
        if path_aware
            && !is_loud_promo
            && matches!(kind, CaptureKind::SimpleTake)
            && !takes_royal
            && net_below_hang_frac(enemy, own, mover_value)
            && state
                .get_board()
                .is_position_attacked_by_color(landing, opponent)
        {
            continue;
        }

        let Some(undo) = state.make_move_for_search(mv) else {
            continue;
        };

        // PathAware post-fire hang for PathClear/MultiLeg (may remove landing defenders).
        if path_aware
            && !is_loud_promo
            && matches!(kind, CaptureKind::PathClear | CaptureKind::MultiLeg)
            && !takes_royal
            && net_below_hang_frac(enemy, own, mover_value)
        {
            if state
                .get_board()
                .is_position_attacked_by_color(landing, opponent)
            {
                state.unmake_move_for_search(undo);
                continue;
            }
        }

        let next_prev_to = if ctx.q_prune_mode.uses_recapture_only() || path_aware {
            Some(landing)
        } else {
            None
        };
        let next_allow_pathclear = allow_pathclear
            && !(ctx.q_no_pathclear_reply
                && matches!(kind, CaptureKind::PathClear | CaptureKind::MultiLeg));

        ctx.q_caps_searched += 1;
        match kind {
            CaptureKind::SimpleTake => ctx.q_kind_simple += 1,
            CaptureKind::PathClear => ctx.q_kind_path += 1,
            CaptureKind::MultiLeg => ctx.q_kind_multi += 1,
        }
        ctx.ply = parent_ply + 1;
        let score = -quiesce(
            state,
            weights,
            qdepth - 1,
            -beta,
            -alpha,
            next_prev_to,
            next_allow_pathclear,
            true,
            ctx,
        );
        state.unmake_move_for_search(undo);
        ctx.ply = parent_ply;
        ctx.q_depth_left = qdepth;
        ctx.q_caps_at_node = n_caps;

        if score > best {
            best = score;
            best_move_key = Some(mv_key);
        }
        if score > alpha {
            alpha = score;
        }
        if alpha >= beta {
            break;
        }
    }

    let bound = if best <= alpha_orig {
        TtBound::Upper
    } else if best >= beta {
        TtBound::Lower
    } else {
        TtBound::Exact
    };
    ctx.q_tt.store(TtEntry {
        key,
        depth: qdepth,
        score: best,
        bound,
        best: best_move_key,
    });
    best
}

/// True if this capture takes an enemy on `sq` (landing, intermediate, or path).
pub(crate) fn capture_hits_square(state: &GameState, mv: &Move, sq: Position) -> bool {
    if mv.to == sq {
        return move_captures_enemy_raw(state, mv);
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return false;
    };
    let them = piece.color.opposite();
    let is_enemy = |pos: Position| {
        board
            .get_piece(pos)
            .map(|p| p.color == them)
            .unwrap_or(false)
    };
    if let Some(inter) = mv.intermediate() {
        if inter == sq && is_enemy(sq) {
            return true;
        }
    }
    if let Some(path) = mv.free_eagle_path() {
        return path.iter().any(|p| *p == sq && is_enemy(sq));
    }
    let config = MovementConfig::for_piece(&piece);
    let uses_capturing = config.capabilities.iter().any(|cap| {
        matches!(
            cap,
            MovementCapability::Range {
                blocking: BlockingMode::Capturing,
                ..
            }
        )
    });
    if uses_capturing {
        return path_utils::get_path_positions(mv.from, mv.to)
            .into_iter()
            .any(|p| p == sq && p != mv.from && is_enemy(sq));
    }
    false
}

/// Like alphabeta but records reply nodes for the GUI (best-move expansion only).
fn alphabeta_record(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    ctx: &mut SearchContext,
) -> (i32, Option<SearchTreeNode>) {
    ctx.nodes += 1;
    let static_eval = evaluate_with_ply(state, weights, ctx.ply);

    if state.get_winner().is_some() || ctx.timed_out() {
        return (
            static_eval,
            Some(SearchTreeNode {
                label: "eval".into(),
                score: Some(static_eval),
                static_eval: Some(static_eval),
                best: true,
                cutoff: false,
                children: vec![],
            }),
        );
    }
    if depth == 0 {
        let score = leaf_or_quiesce(state, weights, i32::MIN + 1, i32::MAX - 1, true, ctx);
        return (
            score,
            Some(SearchTreeNode {
                label: "eval".into(),
                score: Some(score),
                static_eval: Some(static_eval),
                best: true,
                cutoff: false,
                children: vec![],
            }),
        );
    }

    let mut moves = state.generate_legal_moves();
    if moves.is_empty() {
        return (static_eval, None);
    }

    order_moves(state, weights, &mut moves);

    let mut best = i32::MIN + 1;
    let mut best_label: Option<String> = None;
    let mut children: Vec<SearchTreeNode> = Vec::new();
    let parent_ply = ctx.ply;
    let mut alpha = i32::MIN + 1;
    let beta = i32::MAX - 1;

    for mv in moves {
        if ctx.timed_out() {
            break;
        }
        let label = move_label(state, &mv);
        let is_capture = move_captures_enemy(state, &mv);
        let capture_enemy = move_loudness(state, weights, &mv, is_capture);
        let is_wipe = quiesce_move_looks_path_or_multileg(state, &mv);
        let mover_large = mover_is_large(state, &mv);
        let Some(undo) = state.make_move_for_search(mv.clone()) else {
            continue;
        };
        ctx.last_ab_capture_enemy = capture_enemy;
        ctx.last_ab_to = Some(mv.to);
        ctx.last_ab_wipe = is_wipe;
        ctx.last_ab_mover_large = mover_large;
        ctx.ply = parent_ply + 1;
        let score = -alphabeta(state, weights, depth - 1, -beta, -alpha, true, ctx);
        state.unmake_move_for_search(undo);
        ctx.ply = parent_ply;

        if score > best {
            best = score;
            best_label = Some(label.clone());
        }
        if score > alpha {
            alpha = score;
        }
        let cutoff = alpha >= beta;
        children.push(SearchTreeNode {
            label,
            score: Some(score),
            static_eval: None,
            best: false,
            cutoff,
            children: vec![],
        });
        if cutoff {
            break;
        }
    }

    if let Some(ref bl) = best_label {
        for c in &mut children {
            if &c.label == bl {
                c.best = true;
            }
        }
    }
    children.sort_by(|a, b| {
        b.best.cmp(&a.best).then(
            b.score
                .unwrap_or(i32::MIN)
                .cmp(&a.score.unwrap_or(i32::MIN)),
        )
    });
    if children.len() > MAX_TREE_BRANCH {
        children.truncate(MAX_TREE_BRANCH);
    }

    (
        best,
        Some(SearchTreeNode {
            label: "replies".into(),
            score: Some(best),
            static_eval: Some(static_eval),
            best: true,
            cutoff: false,
            children,
        }),
    )
}

impl SearchContext {
    /// Log progress about every 3s while a search is still running.
    fn maybe_log_progress(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_progress_log) < Duration::from_secs(3) {
            return;
        }
        self.last_progress_log = now;
        let elapsed = now.duration_since(self.search_started);
        let ms = elapsed.as_millis().max(1) as u64;
        let nps = self.nodes.saturating_mul(1000) / ms;
        let best = if self.best_score > i32::MIN + 1 {
            format!("{}", self.best_score)
        } else {
            "-".into()
        };
        let root = if self.root_total > 0 {
            format!(
                "{}/{} {}",
                self.root_index, self.root_total, self.root_label
            )
        } else {
            "-".into()
        };
        let q_this = self.q_nodes.saturating_sub(self.q_nodes_at_root_start);
        let qpct = if self.nodes > 0 {
            (self.q_nodes.saturating_mul(100)) / self.nodes
        } else {
            0
        };
        let fh_pct = if self.root_pvs_tried == 0 {
            0
        } else {
            (self.root_fail_high.saturating_mul(100)) / self.root_pvs_tried
        };
        let qhit_pct = if self.q_tt_probes == 0 {
            0
        } else {
            (self.q_tt_hits.saturating_mul(100)) / self.q_tt_probes
        };
        let qinfo = if self.phase == "quiesce" || self.q_nodes > 0 {
            format!(
                " qnodes={} q%={} qroot={} qlast={} quniq={} qTThit={}% fh={}% qPC/ST/ML={}/{}/{} qleft={} qcaps={}/{} qmv={} spat={}",
                self.q_nodes,
                qpct,
                q_this,
                self.q_nodes_last_root,
                self.q_unique.len(),
                qhit_pct,
                fh_pct,
                self.q_kind_path,
                self.q_kind_simple,
                self.q_kind_multi,
                self.q_depth_left,
                self.q_cap_index,
                self.q_caps_at_node,
                if self.q_label.is_empty() {
                    "-"
                } else {
                    &self.q_label
                },
                self.q_stand_pat
            )
        } else {
            String::new()
        };
        let rms = now.duration_since(self.root_move_started).as_millis();
        eprintln!(
            "ab search: {:.1}s nodes={} nps={} depth={} q={} phase={} root={} best={} rms={}{}",
            elapsed.as_secs_f64(),
            self.nodes,
            nps,
            self.search_depth,
            self.quiescence_depth,
            self.phase,
            root,
            best,
            rms,
            qinfo
        );
    }

    fn timed_out(&mut self) -> bool {
        // Cheap throttle: don't Instant::now on every node.
        if self.nodes & 0xff == 0 {
            self.maybe_log_progress();
        }
        if self.abort {
            return true;
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                self.abort = true;
                return true;
            }
        }
        false
    }
}

fn order_moves(state: &GameState, weights: &EvalWeights, moves: &mut [Move]) {
    let opponent = state.get_current_turn().opposite();
    let mut attack_cache = LandingAttackCache::new();
    moves.sort_by_key(|mv| {
        std::cmp::Reverse(move_order_score(
            state,
            weights,
            mv,
            opponent,
            &mut attack_cache,
            false,
        ))
    });
}

/// Main-search ordering: captures (hang/MVV) then killers then history for quiets.
///
/// `postfire_pathclear_hang`: PathClear/MultiLeg hang uses post-fire directly when
/// true (root); interior uses confirm-on-prune (cheap pre-move, sim only if needed).
fn order_moves_with_heuristics(
    state: &GameState,
    weights: &EvalWeights,
    moves: &mut [Move],
    ctx: &SearchContext,
    ply: usize,
    captures_only_style: bool,
    postfire_pathclear_hang: bool,
) {
    #[cfg(feature = "search-profile")]
    let _ord = crate::profile_timers::order_scope();
    let opponent = state.get_current_turn().opposite();
    if captures_only_style {
        moves.sort_by_key(|mv| {
            let cap = mvv_lva_score(state, weights, mv);
            let kr = killer_rank(ctx, ply, mv);
            let hist = history_score(ctx, mv);
            std::cmp::Reverse((cap, kr, hist))
        });
        return;
    }
    let mut attack_cache = LandingAttackCache::new();
    moves.sort_by_key(|mv| {
        let cap = move_order_score(
            state,
            weights,
            mv,
            opponent,
            &mut attack_cache,
            postfire_pathclear_hang,
        );
        let kr = killer_rank(ctx, ply, mv);
        let hist = history_score(ctx, mv);
        std::cmp::Reverse((cap, kr, hist))
    });
}

/// Test/helper: ordering score with a fresh per-call attack cache.
#[cfg(test)]
fn move_order_score_fresh(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let opponent = state.get_current_turn().opposite();
    let mut cache = LandingAttackCache::new();
    move_order_score(state, weights, mv, opponent, &mut cache, true)
}

fn move_label(state: &GameState, mv: &Move) -> String {
    let board = state.get_board();
    let sym = board
        .get_piece(mv.from)
        .map(|p| {
            let s = p.base_symbol();
            if p.is_promoted {
                format!("+{}", s)
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "?".into());
    let promo = if mv.promoted { "+" } else { "" };
    format!(
        "{} {},{}→{},{}{}",
        sym,
        mv.from.file + 1,
        mv.from.rank + 1,
        mv.to.file + 1,
        mv.to.rank + 1,
        promo
    )
}

/// Convert a search result into a GUI/API payload.
pub fn search_info_from_result(
    agent: &str,
    side: &str,
    depth: u32,
    result: &SearchResult,
) -> SearchInfo {
    let best_move = result
        .tree
        .children
        .iter()
        .find(|c| c.best)
        .map(|c| c.label.clone())
        .or_else(|| {
            result.best_move.as_ref().map(|mv| {
                format!(
                    "{},{}→{},{}",
                    mv.from.file + 1,
                    mv.from.rank + 1,
                    mv.to.file + 1,
                    mv.to.rank + 1
                )
            })
        });

    let root_moves = if result.tree.children.is_empty() {
        result
            .root_lines
            .iter()
            .take(MAX_TREE_ROOT_CHILDREN)
            .enumerate()
            .map(|(i, (mv, score))| RootMoveInfo {
                label: format!(
                    "{},{}→{},{}",
                    mv.from.file + 1,
                    mv.from.rank + 1,
                    mv.to.file + 1,
                    mv.to.rank + 1
                ),
                score: *score,
                best: i == 0,
            })
            .collect()
    } else {
        result
            .tree
            .children
            .iter()
            .map(|c| RootMoveInfo {
                label: c.label.clone(),
                score: c.score.unwrap_or(0),
                best: c.best,
            })
            .collect()
    };

    SearchInfo {
        agent: agent.to_string(),
        side: side.to_string(),
        depth,
        nodes: result.nodes,
        static_eval: result.static_eval,
        score: result.score,
        best_move,
        root_moves,
        tree: result.tree.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::EvalWeights;
    use crate::piece::{Color, Piece, PieceType};
    use crate::position::Position;

    #[test]
    fn wipe_group_key_empty_beyond_shares_occupied_set() {
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 14).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let from = Position::new(10, 10).unwrap();
        let park_a = Move::new(from, Position::new(10, 16).unwrap());
        let park_b = Move::new(from, Position::new(10, 18).unwrap());
        let on_second = Move::new(from, Position::new(10, 14).unwrap());
        let on_first = Move::new(from, Position::new(10, 12).unwrap());
        let ka = capturing_wipe_group_key(&state, &park_a);
        let kb = capturing_wipe_group_key(&state, &park_b);
        assert!(ka.is_some(), "empty-beyond should group");
        assert_eq!(ka, kb, "same occupied-between set");
        assert!(
            capturing_wipe_group_key(&state, &on_second).is_none(),
            "landing on a victim is full-search"
        );
        assert!(
            capturing_wipe_group_key(&state, &on_first).is_none(),
            "landing on first victim is full-search"
        );
    }

    #[test]
    fn depth_one_prefers_capturing_lone_royal() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(20, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: true,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("expected a move");
        assert_eq!(best.to, Position::new(10, 11).unwrap());
        assert!(
            result.score > 100_000,
            "mate-ish score, got {}",
            result.score
        );
        assert!(!result.tree.children.is_empty());
    }

    #[test]
    fn play_and_trace_agree_on_best_move() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        // Small board so depth-1 + qsearch stays cheap.
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let cfg_play = SearchConfig {
            depth: 1,
            collect_trace: false,
            quiescence_depth: 4,
            q_prune_mode: QPruneMode::PathAware,
            max_time_ms: None,
            ..Default::default()
        };
        let mut cfg_trace = cfg_play.clone();
        cfg_trace.collect_trace = true;
        let play = search(&state, &weights, &cfg_play);
        let traced = search(&state, &weights, &cfg_trace);
        assert_eq!(
            play.best_move.as_ref().map(|m| (m.from, m.to, m.promoted)),
            traced
                .best_move
                .as_ref()
                .map(|m| (m.from, m.to, m.promoted))
        );
        assert_eq!(play.score, traced.score);
    }

    #[test]
    fn search_apply_matches_make_move_on_opening_moves() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let moves = state.generate_legal_moves();
        // Spot-check a spread of opening moves (includes various piece types).
        for mv in moves.iter().step_by(17).take(20) {
            let mut a = state.clone();
            let mut b = state.clone();
            let undo = a.make_move_for_search(mv.clone());
            let _ = b.make_move(mv.clone());
            assert!(undo.is_some(), "search apply failed for {:?}", mv);
            assert_eq!(a.get_current_turn(), b.get_current_turn());
            assert_eq!(
                a.get_turns_without_capture_or_promotion(),
                b.get_turns_without_capture_or_promotion()
            );
            for file in 0..36u8 {
                for rank in 0..36u8 {
                    let pos = Position::new(file, rank).unwrap();
                    assert_eq!(
                        a.get_board().get_piece(pos),
                        b.get_board().get_piece(pos),
                        "board mismatch after {:?}",
                        mv
                    );
                }
            }
            a.unmake_move_for_search(undo.unwrap());
            assert_eq!(a.get_current_turn(), state.get_current_turn());
            assert_eq!(
                a.get_turns_without_capture_or_promotion(),
                state.get_turns_without_capture_or_promotion()
            );
            for file in 0..36u8 {
                for rank in 0..36u8 {
                    let pos = Position::new(file, rank).unwrap();
                    assert_eq!(
                        a.get_board().get_piece(pos),
                        state.get_board().get_piece(pos),
                        "unmake mismatch after {:?}",
                        mv
                    );
                }
            }
        }
    }

    #[test]
    fn opening_depth2_play_search_completes() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let t0 = Instant::now();
        // Release: d2/q2 target after capture-gen + staging + TT (allow small machine slack).
        #[cfg(debug_assertions)]
        let qdepth = 0u32;
        #[cfg(not(debug_assertions))]
        let qdepth = 2u32;
        #[cfg(debug_assertions)]
        let max_secs = 10u64;
        #[cfg(not(debug_assertions))]
        let max_secs_f = 2.5f64;
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 2,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: qdepth,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let elapsed = t0.elapsed();
        assert!(result.best_move.is_some());
        assert!(result.nodes > 0);
        #[cfg(debug_assertions)]
        assert!(
            elapsed.as_secs() < max_secs,
            "opening depth-2 q{qdepth} ID took {:?}, nodes={}",
            elapsed,
            result.nodes
        );
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_secs_f64() < max_secs_f,
            "opening depth-2 q{qdepth} ID took {:?}, nodes={}",
            elapsed,
            result.nodes
        );
        assert!(
            result.score > -5_000,
            "opening ID score unexpectedly bad: {}",
            result.score
        );
        eprintln!(
            "opening depth-2 q{qdepth} ID: {:?} nodes={} score={}",
            elapsed, result.nodes, result.score
        );
    }

    #[test]
    fn opening_depth3_q2_completes_quickly_release() {
        // Selective search (null/LMR/killers) should make d3 interactive in release.
        #[cfg(debug_assertions)]
        {
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            let weights = EvalWeights::seed();
            let mut state = GameState::new();
            state.setup_initial_position();
            let t0 = Instant::now();
            let result = search(
                &state,
                &weights,
                &SearchConfig {
                    depth: 3,
                    max_time_ms: None,
                    collect_trace: false,
                    quiescence_depth: 2,
                    q_prune_mode: QPruneMode::PathAware,
                    ..Default::default()
                },
            );
            let elapsed = t0.elapsed();
            assert!(result.best_move.is_some());
            assert!(
                elapsed.as_secs_f64() < 2.5,
                "opening d3/q2 took {:?}, nodes={}",
                elapsed,
                result.nodes
            );
            eprintln!(
                "opening d3/q2: {:?} nodes={} score={}",
                elapsed, result.nodes, result.score
            );
        }
    }

    #[test]
    fn pathaware_quiesce_budgets_are_tight() {
        assert_eq!(QUIESCE_TOP_N_PATH_AWARE_ROOT, 2);
        assert_eq!(QUIESCE_TOP_N_DEEP, 3);
        assert_eq!(QUIESCE_PATHCLEAR_DEEP_BUDGET, 1);
    }

    #[test]
    fn pathclear_q_allows_dest_recapture_only() {
        assert!(!pathclear_allowed_in_pathaware_q(false));
        assert!(pathclear_allowed_in_pathaware_q(true));
    }

    #[test]
    fn pathclear_mop_and_loud_landing_need_dest_recapture() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Pawn, 1.0);
        weights.piece.insert(PieceType::GoldGeneral, 2000.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(12, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(12, 14).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let mop = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 14).unwrap(),
        );
        let loud_land = Move::new(
            Position::new(12, 10).unwrap(),
            Position::new(12, 14).unwrap(),
        );
        let (_, _, mop_kind) = capture_exchange_kind(&state, &weights, &mop);
        assert_eq!(mop_kind, CaptureKind::PathClear);
        assert!(quiesce_move_looks_path_or_multileg(&state, &mop));
        // Empty path to the gold is a SimpleTake (not PathClear) — still OK.
        let (_, _, land_kind) = capture_exchange_kind(&state, &weights, &loud_land);
        assert_eq!(land_kind, CaptureKind::SimpleTake);
        // Neither mop nor non-dest PathClear policy admits without dest recapture.
        assert!(!pathclear_allowed_in_pathaware_q(false));
        assert!(pathclear_allowed_in_pathaware_q(prev_to_is_dest(
            Some(loud_land.to),
            &loud_land
        )));
        // Generate-time filter: mop PathClear absent without dest prev_to.
        let gen_none =
            generate_quiescence_captures(&state, &weights, None, false, true, true, false);
        assert!(!gen_none.iter().any(|m| same_root_move(m, &mop)));
        // Loud SimpleTake clears the floor and is kept even without prev_to.
        assert!(gen_none.iter().any(|m| same_root_move(m, &loud_land)));
        let gen_dest = generate_quiescence_captures(
            &state,
            &weights,
            Some(loud_land.to),
            false,
            true,
            true,
            false,
        );
        assert!(gen_dest.iter().any(|m| same_root_move(m, &loud_land)));
        assert!(!gen_dest.iter().any(|m| same_root_move(m, &mop)));
    }

    fn prev_to_is_dest(prev_to: Option<Position>, mv: &Move) -> bool {
        prev_to == Some(mv.to)
    }

    /// Advance from the opening with deterministic quiet pawn pushes to get
    /// a busier midgame-ish branching factor for smoke timing.
    fn midgame_ish_after_pawn_pushes() -> GameState {
        let mut state = GameState::new();
        state.setup_initial_position();
        // Push several black/white pawns one step each when legal.
        for file in [4u8, 8, 12, 16, 20, 24, 28] {
            for &(from_rank, to_rank, color) in
                &[(10u8, 11u8, Color::Black), (25u8, 24u8, Color::White)]
            {
                state.set_current_turn(color);
                let from = Position::new(file, from_rank).unwrap();
                let to = Position::new(file, to_rank).unwrap();
                if state.get_board().get_piece(from).map(|p| p.piece_type) == Some(PieceType::Pawn)
                    && state.get_board().get_piece(to).is_none()
                {
                    let mv = Move::new(from, to);
                    let _ = state.make_move_for_search(mv);
                }
            }
        }
        state.set_current_turn(Color::Black);
        state
    }

    #[test]
    fn midgame_d4_q2_and_q4_smoke_release() {
        #[cfg(debug_assertions)]
        {
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            let weights = EvalWeights::seed();
            let state = midgame_ish_after_pawn_pushes();
            let root_n = state.generate_legal_moves().len();
            assert!(root_n > 200, "expected busy root, got {root_n}");

            let t0 = Instant::now();
            let q2 = search(
                &state,
                &weights,
                &SearchConfig {
                    depth: 4,
                    max_time_ms: Some(12_000),
                    collect_trace: false,
                    quiescence_depth: 2,
                    q_prune_mode: QPruneMode::PathAware,
                    ..Default::default()
                },
            );
            let q2_ms = t0.elapsed().as_millis();

            let t1 = Instant::now();
            let q4 = search(
                &state,
                &weights,
                &SearchConfig {
                    depth: 4,
                    max_time_ms: Some(12_000),
                    collect_trace: false,
                    quiescence_depth: 4,
                    q_prune_mode: QPruneMode::PathAware,
                    ..Default::default()
                },
            );
            let q4_ms = t1.elapsed().as_millis();

            assert!(q2.best_move.is_some());
            assert!(q4.best_move.is_some());
            eprintln!(
                "midgame smoke root={root_n}: d4/q2 {}ms nodes={} qnodes={} | d4/q4 {}ms nodes={} qnodes={}",
                q2_ms, q2.nodes, q2.q_nodes, q4_ms, q4.nodes, q4.q_nodes
            );
            // Soft budgets: should make progress well under the wall, not hang.
            assert!(
                q2_ms < 12_500,
                "d4/q2 exceeded soft wall: {q2_ms}ms nodes={}",
                q2.nodes
            );
            assert!(
                q4.nodes > 500,
                "d4/q4 made too little progress: nodes={}",
                q4.nodes
            );
        }
    }

    #[test]
    fn midgame_d4_q_ablation_release() {
        // Separates root-width cost (q0) from quiescence cost (q2/q4).
        #[cfg(debug_assertions)]
        {
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            let weights = EvalWeights::seed();
            let state = midgame_ish_after_pawn_pushes();
            let budget = Some(8_000u64);
            let mut rows = Vec::new();
            for q in [0u32, 2, 4] {
                let t0 = Instant::now();
                let r = search(
                    &state,
                    &weights,
                    &SearchConfig {
                        depth: 4,
                        max_time_ms: budget,
                        collect_trace: false,
                        quiescence_depth: q,
                        q_prune_mode: QPruneMode::PathAware,
                        ..Default::default()
                    },
                );
                let ms = t0.elapsed().as_millis();
                eprintln!(
                    "ablation d4/q{q}: {ms}ms nodes={} qnodes={} score={} best={:?}",
                    r.nodes,
                    r.q_nodes,
                    r.score,
                    r.best_move.as_ref().map(|m| (m.from, m.to))
                );
                assert!(r.best_move.is_some());
                rows.push((q, ms, r.nodes, r.q_nodes));
            }
            // q0 should not be dominated by qnodes.
            assert_eq!(rows[0].0, 0);
            assert!(
                rows[0].3 < rows[0].2 / 2 || rows[0].3 < 1_000,
                "q0 should spend little in quiescence: nodes={} qnodes={}",
                rows[0].2,
                rows[0].3
            );
        }
    }

    #[test]
    fn opening_depth4_makes_root_progress_in_budget_release() {
        // With a few seconds, ID should finish d3 and start several d4 root moves.
        #[cfg(debug_assertions)]
        {
            return;
        }
        #[cfg(not(debug_assertions))]
        {
            let weights = EvalWeights::seed();
            let mut state = GameState::new();
            state.setup_initial_position();
            let result = search(
                &state,
                &weights,
                &SearchConfig {
                    depth: 4,
                    max_time_ms: Some(8_000),
                    collect_trace: false,
                    quiescence_depth: 2,
                    q_prune_mode: QPruneMode::PathAware,
                    ..Default::default()
                },
            );
            assert!(result.best_move.is_some());
            assert!(
                result.nodes > 50_000,
                "expected meaningful d4 progress, nodes={}",
                result.nodes
            );
            // Completing d3 (~273k before reductions) or a large d4 partial both count.
            eprintln!(
                "opening d4/q2 @8s: nodes={} score={} best={:?}",
                result.nodes,
                result.score,
                result.best_move.as_ref().map(|m| (m.from, m.to))
            );
        }
    }

    #[test]
    fn capture_gen_faster_than_full_on_opening() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let full_n = state.generate_legal_moves().len();
        let caps_n = state
            .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
            .len();
        assert!(caps_n < full_n, "captures_only={caps_n} full={full_n}");
        // Timing is noisy in debug; only assert speedup in release.
        #[cfg(not(debug_assertions))]
        {
            let t0 = Instant::now();
            for _ in 0..50 {
                let _ = state.generate_legal_moves();
            }
            let full = t0.elapsed();
            let t1 = Instant::now();
            for _ in 0..50 {
                let _ = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
            }
            let caps = t1.elapsed();
            eprintln!("opening gen x50: full={full:?} captures_only={caps:?}");
            assert!(
                caps <= full,
                "captures_only should not be slower: {caps:?} vs {full:?}"
            );
        }
    }

    #[test]
    fn capture_gen_keeps_capturing_two_step_omits_quiet() {
        // Lion: simple×simple two-step — easy to place a capture vs quiet legs.
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Lion,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        // Enemy on a one-step square so first-leg / two-step can capture.
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let caps = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
        let quiet_ml = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);
        let all = state.generate_legal_moves();

        assert!(
            caps.iter()
                .any(|m| m.is_two_step() && move_captures_enemy(&state, m)),
            "CapturesOnly should include a capturing two-step"
        );
        assert!(
            caps.iter().all(|m| move_captures_enemy(&state, m)),
            "CapturesOnly must not emit quiets"
        );
        assert!(
            quiet_ml.iter().any(|m| m.is_two_step()),
            "QuietMultiLegOnly should still find quiet two-steps on an open Lion"
        );
        assert!(
            quiet_ml
                .iter()
                .all(|m| m.is_two_step() || m.is_free_eagle()),
            "QuietMultiLegOnly should only be multi-leg"
        );
        assert!(
            quiet_ml.iter().all(|m| !move_captures_enemy(&state, m)),
            "QuietMultiLegOnly must omit captures"
        );
        assert!(
            all.len() > caps.len(),
            "full gen should exceed capture-only"
        );
    }

    #[test]
    fn id_timeout_returns_last_completed_depth() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let mut state = GameState::new();
        state.setup_initial_position();
        // Budget must allow at least depth-1 root progress; PathClear ordering
        // sims add cost on the opening.
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 2,
                max_time_ms: Some(2_000),
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert!(result.best_move.is_some());
        assert!(!result.root_lines.is_empty());
    }

    #[test]
    fn id_tiny_budget_high_ceiling_returns_a_move() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let mut state = GameState::new();
        state.setup_initial_position();
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 8,
                max_time_ms: Some(5),
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert!(result.best_move.is_some());
    }

    #[test]
    fn reorder_root_moves_puts_best_first() {
        let a = Move::new(Position::new(1, 1).unwrap(), Position::new(1, 2).unwrap());
        let b = Move::new(Position::new(2, 1).unwrap(), Position::new(2, 2).unwrap());
        let c = Move::new(Position::new(3, 1).unwrap(), Position::new(3, 2).unwrap());
        let mut moves = vec![a.clone(), b.clone(), c.clone()];
        let scored = vec![(a.clone(), 1), (b.clone(), 5), (c.clone(), 3)];
        reorder_root_moves(&mut moves, &b, &scored);
        assert!(same_root_move(&moves[0], &b));
        assert!(same_root_move(&moves[1], &c));
        assert!(same_root_move(&moves[2], &a));
    }

    #[test]
    fn recapture_below_floor_is_quiescence_candidate() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Pawn, 1.0);
        weights.piece.insert(PieceType::GoldGeneral, 50.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        let landing = Position::new(10, 11).unwrap();
        assert!(!is_worthwhile_quiescence_capture(&state, &weights, &mv));
        assert!(is_quiescence_capture_candidate(
            &state,
            &weights,
            &mv,
            Some(landing)
        ));
        assert!(!is_quiescence_capture_candidate(
            &state, &weights, &mv, None
        ));
        let gen =
            generate_quiescence_captures(&state, &weights, Some(landing), false, true, true, false);
        assert!(gen.iter().any(|m| same_root_move(m, &mv)));
    }

    #[test]
    fn capturable_max_gain_ignores_untouchable_board_pieces() {
        // Living GG on the board must not inflate the optimistic bound when the
        // only capturable gain is a smaller loud take.
        let gains_with_only_gold = [2000.0f32];
        let best = gains_with_only_gold
            .iter()
            .copied()
            .fold(0.0f32, f32::max)
            .round() as i32;
        assert_eq!(best, 2000);
        // Board-max would be ~GG (4000+); capturable-max stays at the gold take.
        assert!(best < 3500);
    }

    #[test]
    fn path_sum_outranks_landing_victim_in_sort_key() {
        // Sort key primary is total captured (path-sum / tactical_gain): a 3000
        // corridor wipe beats a 4000 landing that took only 500 along the way.
        let key = |tactical_gain: f32, dest_recapture: bool, recapture: bool, net_lva: i32| {
            (
                (tactical_gain * 1000.0).round() as i32,
                dest_recapture,
                recapture,
                net_lva,
            )
        };
        assert!(key(3000.0, false, false, 0) > key(500.0, false, false, 0));
        // Equal loot: dest recapture of the contested square wins the tie.
        assert!(key(2160.0, true, true, 0) > key(2160.0, false, true, 0));
    }

    #[test]
    fn root_lmr_reduces_quiets_not_captures() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Pawn, 1.0);
        weights.piece.insert(PieceType::GoldGeneral, 2000.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(5, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let capture = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        let quiet = Move::new(Position::new(5, 10).unwrap(), Position::new(5, 11).unwrap());
        assert!(move_captures_enemy(&state, &capture));
        assert!(!move_captures_enemy(&state, &quiet));
        // Pre–PR17 root LMR: late quiets reducible; any capture is not.
        let can_reduce_capture = false; // !is_capture
        let can_reduce_quiet = true;
        assert!(!can_reduce_capture);
        assert!(can_reduce_quiet);
    }

    #[test]
    fn deep_id_searches_full_root_not_narrowed() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let moves = state.generate_legal_moves();
        let n_legal = moves.len();
        assert!(n_legal > 64, "opening should have a wide root");
        let mut hang_cache = LandingAttackCache::new();
        let hang_skipped = moves
            .iter()
            .filter(|mv| {
                move_captures_enemy(&state, mv)
                    && capture_hangs_high_value_piece(&state, &weights, mv, true, &mut hang_cache)
            })
            .count();
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: true,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        // Depth 1 finishes the full searchable root (hanging high-value captures skipped).
        assert!(
            result.root_lines.len() > 64,
            "expected full-root coverage, got {}",
            result.root_lines.len()
        );
        assert_eq!(result.root_lines.len(), n_legal - hang_skipped);
    }

    #[test]
    fn dest_capture_detected_self_capture_excluded() {
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let capture = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        assert!(move_captures_enemy(&state, &capture));

        let self_cap = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 10).unwrap(),
        );
        assert!(!move_captures_enemy(&state, &self_cap));
    }

    #[test]
    fn capturing_range_path_capture_detected() {
        let mut state = GameState::new();
        // Great General: capturing-range in all directions.
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        // Land beyond the pawn on empty square — path capture only.
        let sweep = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 14).unwrap(),
        );
        assert!(move_captures_enemy(&state, &sweep));
        let quiet = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(12, 10).unwrap(),
        );
        assert!(!move_captures_enemy(&state, &quiet));
    }

    #[test]
    fn quiescence_floors_track_seed_loud_capture_floor() {
        let floor = seed_loud_capture_floor();
        assert!((floor - 648.0).abs() < 1e-3);
        assert!((min_quiescence_enemy_material() - floor).abs() < 1e-6);
        assert!((min_quiescence_deep_enemy() - floor).abs() < 1e-6);
        // Hang when net is below HANG_NET_FRAC of mover (even if enemy_sum is large).
        assert!(net_below_hang_frac(1500.0, 0.0, 4000.0)); // 1500 < 3200
        assert!(!net_below_hang_frac(3500.0, 0.0, 4000.0)); // healthy PathClear net
        assert!(net_below_hang_frac(2000.0, 500.0, 4000.0)); // net 1500 < 3200
    }

    #[test]
    fn quiescence_skips_low_value_enemy_captures() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        // Seed pawn value is 1 — below the loud-capture floor.
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 14).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 14).unwrap(),
        );
        assert!(move_captures_enemy(&state, &mv));
        assert!(
            !is_worthwhile_quiescence_capture(&state, &weights, &mv),
            "taking a low-value pawn should not enter qsearch"
        );
    }

    #[test]
    fn quiescence_includes_big_piece_captures() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        assert!(is_worthwhile_quiescence_capture(&state, &weights, &mv));
    }

    /// Cheap attacker vs adjacent GG on a file — Gold steps one orthogonally.
    fn hung_gg_by_gold() -> (GameState, EvalWeights, Move) {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        (state, weights, mv)
    }

    #[test]
    fn large_hang_simple_take_detects_lesser_attacker() {
        let (state, weights, mv) = hung_gg_by_gold();
        assert!(is_large_hang_simple_take(&state, &weights, &mv));
        assert!(stm_has_large_hang_simple_take(&state, &weights));
        let gen = generate_quiescence_captures(&state, &weights, None, false, true, true, false);
        assert!(
            gen.iter().any(|m| m.from == mv.from && m.to == mv.to),
            "q gen must include Gold×GG hang take: {gen:?}"
        );
    }

    #[test]
    fn large_hang_ignores_equal_or_higher_attacker() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        // Two GreatGenerals facing — equal trade, not a "hang" under lesser-attacker rule.
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        assert!(!is_large_hang_simple_take(&state, &weights, &mv));
        assert!(!stm_has_large_hang_simple_take(&state, &weights));
    }

    #[test]
    fn quiet_parent_leaf_enters_q_for_large_hang_take() {
        let (state, weights, _mv) = hung_gg_by_gold();
        let stand = evaluate_with_ply(&state, &weights, 0);
        let (score, q_nodes) = probe_quiet_parent_leaf_or_quiesce(&state, &weights, 2);
        assert!(
            q_nodes > 0,
            "quiet parent must open q when a large hang take exists"
        );
        assert!(
            score > stand,
            "taking hung GG in q should beat stand-pat: stand={stand} q={score}"
        );
    }

    #[test]
    fn q_tries_dest_recapture_of_major_before_stand_pat_cutoff() {
        let (state, weights, take) = hung_gg_by_gold();
        let stand = evaluate_with_ply(&state, &weights, 0);
        let (score, q_nodes) =
            probe_quiesce_window(&state, &weights, 2, stand - 50, stand - 1, Some(take.to));
        assert!(
            q_nodes > 1,
            "must expand dest recapture, not cut at stand-pat"
        );
        assert!(
            score > stand,
            "taking hung GG must beat stand-pat even when stand-pat >= β: stand={stand} q={score}"
        );
    }

    #[test]
    fn quiet_parent_leaf_stand_pats_without_large_hang() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        // Equal GG trade available — must not force quiet-parent q entry.
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let stand = evaluate_with_ply(&state, &weights, 0);
        let (score, q_nodes) = probe_quiet_parent_leaf_or_quiesce(&state, &weights, 2);
        assert_eq!(q_nodes, 0, "no lesser-valued large hang → no q");
        assert_eq!(score, stand);
    }

    fn hang_q_kings() -> (GameState, EvalWeights) {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        (state, weights)
    }

    /// Peacock two-step landing on a Hook (idea A). No cheap SimpleTake.
    fn peacock_dest_hangs_hook() -> (GameState, EvalWeights, Move) {
        let (mut state, weights) = hang_q_kings();
        state.place_piece(Piece::new(
            PieceType::Peacock,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        // Same rank: not on the Peacock's forward-diagonal Simple/first-leg ray,
        // so the dest take must be a two-step (NE then SE).
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(18, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let hook = Position::new(18, 10).unwrap();
        let hits = generate_captures_hitting_square(&state, hook);
        assert!(
            hits.iter().all(|m| m.to != hook || m.is_two_step()),
            "A fixture must not also have a SimpleTake dest onto the Hook: {hits:?}"
        );
        let mv = hits
            .into_iter()
            .find(|m| m.to == hook && m.is_two_step())
            .expect("Peacock two-step dest onto Hook");
        (state, weights, mv)
    }

    /// Hook two-step landing on the other Hook (equal-value dest MultiLeg).
    fn hook_dest_hangs_hook() -> (GameState, EvalWeights, Move) {
        let (mut state, weights) = hang_q_kings();
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::Black,
            Position::new(10, 20).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(18, 14).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let hook = Position::new(18, 14).unwrap();
        let hits = generate_captures_hitting_square(&state, hook);
        assert!(
            hits.iter().all(|m| m.to != hook || m.is_two_step()),
            "equal-hook fixture must be MultiLeg dest, not SimpleTake: {hits:?}"
        );
        let mv = hits
            .into_iter()
            .find(|m| m.to == hook && m.is_two_step())
            .expect("Hook two-step dest onto Hook");
        (state, weights, mv)
    }

    /// GG PathClear through own pawn, landing on a Hook (idea B).
    fn gg_pathclear_dest_hangs_hook() -> (GameState, EvalWeights, Move) {
        let (mut state, weights) = hang_q_kings();
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(10, 12).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(10, 20).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let hook = Position::new(10, 20).unwrap();
        let mv = generate_captures_hitting_square(&state, hook)
            .into_iter()
            .find(|m| m.to == hook && quiesce_move_looks_path_or_multileg(&state, m))
            .expect("GG PathClear dest onto Hook");
        assert!(!mv.is_two_step());
        (state, weights, mv)
    }

    /// GG PathClear that takes a Hook on the path and lands beyond (not B).
    fn gg_corridor_through_hook() -> (GameState, EvalWeights, Move) {
        let (mut state, weights) = hang_q_kings();
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(10, 12).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(10, 15).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 25).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let hook = Position::new(10, 15).unwrap();
        let dest = Position::new(10, 25).unwrap();
        let mv = generate_captures_hitting_square(&state, hook)
            .into_iter()
            .find(|m| m.to == dest && capture_hits_square(&state, m, hook))
            .expect("GG corridor through Hook landing beyond");
        (state, weights, mv)
    }

    #[test]
    fn dest_multileg_hang_on_by_default() {
        let (state, weights, mv) = peacock_dest_hangs_hook();
        assert!(!is_large_hang_simple_take(&state, &weights, &mv));
        assert!(!stm_has_large_hang_simple_take(&state, &weights));
        let gen = generate_quiescence_captures(&state, &weights, None, false, true, true, false);
        assert!(
            gen.iter()
                .any(|m| m.from == mv.from && m.to == mv.to && m.is_two_step()),
            "default q gen must inject MultiLeg dest-hang"
        );
        assert_eq!(
            dest_hang_kind(&state, &weights, &mv, QHangOpts::default()),
            Some(CaptureKind::MultiLeg)
        );
        assert!(dest_hang_kind(&state, &weights, &mv, QHangOpts::OFF).is_none());
    }

    #[test]
    fn dest_multileg_hang_opens_q_when_a_set() {
        let (state, weights, mv) = peacock_dest_hangs_hook();
        let a = QHangOpts {
            dest_multileg: true,
            dest_pathclear: false,
        };
        assert_eq!(
            dest_hang_kind(&state, &weights, &mv, a),
            Some(CaptureKind::MultiLeg)
        );
        assert!(stm_has_large_hang_take(&state, &weights, a));
        let gen = generate_quiescence_captures_with_hang(
            &state, &weights, None, false, true, true, false, a,
        );
        assert!(
            gen.iter()
                .any(|m| m.from == mv.from && m.to == mv.to && m.is_two_step()),
            "A must inject Peacock dest×Hook: {gen:?}"
        );
        let b_only = QHangOpts {
            dest_multileg: false,
            dest_pathclear: true,
        };
        assert!(dest_hang_kind(&state, &weights, &mv, b_only).is_none());
        let (score, q_nodes) = probe_quiet_parent_leaf_or_quiesce_hang(&state, &weights, 2, a);
        let stand = evaluate_with_ply(&state, &weights, 0);
        assert!(q_nodes > 0, "A must open q");
        assert!(
            score > stand,
            "taking Hook in q should beat stand-pat: stand={stand} q={score}"
        );
    }

    #[test]
    fn dest_multileg_hang_includes_equal_hook_trade() {
        let (state, weights, mv) = hook_dest_hangs_hook();
        let mover = state.get_board().get_piece(mv.from).unwrap();
        let victim = state.get_board().get_piece(mv.to).unwrap();
        assert_eq!(
            material_piece_value(&mover, &weights),
            material_piece_value(&victim, &weights)
        );
        assert_eq!(
            dest_hang_kind(&state, &weights, &mv, QHangOpts::default()),
            Some(CaptureKind::MultiLeg)
        );
        assert!(stm_has_large_hang_take(&state, &weights, QHangOpts::AB));
        let gen = generate_quiescence_captures(&state, &weights, None, false, true, true, false);
        assert!(
            gen.iter()
                .any(|m| m.from == mv.from && m.to == mv.to && m.is_two_step()),
            "q gen must inject Hook dest×Hook: {gen:?}"
        );
        let (score, q_nodes) =
            probe_quiet_parent_leaf_or_quiesce_hang(&state, &weights, 2, QHangOpts::AB);
        let stand = evaluate_with_ply(&state, &weights, 0);
        assert!(q_nodes > 0, "equal hook dest-take must open q");
        assert!(
            score > stand,
            "taking the hanging Hook must beat stand-pat: stand={stand} q={score}"
        );
    }

    #[test]
    fn large_mover_subloud_capture_opens_q_only_with_r() {
        let (mut state, weights) = hang_q_kings();
        let hook_sq = Position::new(18, 18).unwrap();
        state.place_piece(Piece::new(PieceType::HookMover, Color::White, hook_sq));
        state.set_current_turn(Color::Black);
        let gold = weights.piece_value(PieceType::GoldGeneral);
        assert!(gold > 0.0 && gold < min_quiescence_enemy_material());
        assert!(!stm_has_large_hang_take(&state, &weights, QHangOpts::OFF));
        assert!(!stm_has_royal_capture(&state));

        let (_, q0) = probe_capture_parent_leaf_or_quiesce_rs(
            &state,
            &weights,
            2,
            gold,
            Some(hook_sq),
            true,
            false,
            false,
            false,
            false,
        );
        assert_eq!(q0, 0, "sub-loud capture must not open q without R");

        let (_, q_r) = probe_capture_parent_leaf_or_quiesce_rs(
            &state,
            &weights,
            2,
            gold,
            Some(hook_sq),
            true,
            true,
            false,
            false,
            false,
        );
        assert!(
            q_r > 0,
            "R must open q after a large-mover sub-loud capture"
        );

        let (_, q_s1) = probe_capture_parent_leaf_or_quiesce_rs(
            &state,
            &weights,
            2,
            gold,
            Some(hook_sq),
            false,
            false,
            true,
            true,
            false,
        );
        assert!(q_s1 > 0, "S1 must open q after any capture");
    }

    #[test]
    fn dest_pathclear_hang_opens_q_when_b_set() {
        let (state, weights, mv) = gg_pathclear_dest_hangs_hook();
        let b = QHangOpts {
            dest_multileg: false,
            dest_pathclear: true,
        };
        assert_eq!(
            dest_hang_kind(&state, &weights, &mv, b),
            Some(CaptureKind::PathClear)
        );
        assert!(!stm_has_large_hang_simple_take(&state, &weights));
        assert!(stm_has_large_hang_take(&state, &weights, b));
        let a_only = QHangOpts {
            dest_multileg: true,
            dest_pathclear: false,
        };
        assert!(dest_hang_kind(&state, &weights, &mv, a_only).is_none());
        let gen_off = generate_quiescence_captures_with_hang(
            &state,
            &weights,
            None,
            false,
            true,
            true,
            false,
            QHangOpts::OFF,
        );
        assert!(
            !gen_off.iter().any(|m| m.from == mv.from && m.to == mv.to),
            "flags-off q gen must drop PathClear dest-hang"
        );
        let gen = generate_quiescence_captures_with_hang(
            &state, &weights, None, false, true, true, false, b,
        );
        assert!(
            gen.iter().any(|m| m.from == mv.from && m.to == mv.to),
            "B must inject GG dest×Hook"
        );
        let (score, q_nodes) = probe_quiet_parent_leaf_or_quiesce_hang(&state, &weights, 2, b);
        let stand = evaluate_with_ply(&state, &weights, 0);
        assert!(q_nodes > 0, "B must open q");
        assert!(
            score > stand,
            "taking Hook in q should beat stand-pat: stand={stand} q={score}"
        );
    }

    #[test]
    fn dest_pathclear_hang_does_not_admit_corridor() {
        let (state, weights, mv) = gg_corridor_through_hook();
        let b = QHangOpts {
            dest_multileg: false,
            dest_pathclear: true,
        };
        assert!(dest_hang_kind(&state, &weights, &mv, b).is_none());
        let gen = generate_quiescence_captures_with_hang(
            &state, &weights, None, false, true, true, false, b,
        );
        assert!(
            !gen.iter().any(|m| m.from == mv.from && m.to == mv.to),
            "B must not inject dest-empty / dest-beyond corridor wipes"
        );
    }

    #[test]
    fn opening_q_gen_drops_mop_pathclear_without_dest_prev() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let caps_only = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
        let pathish = caps_only
            .iter()
            .filter(|m| quiesce_move_looks_path_or_multileg(&state, m))
            .count();
        let gen = generate_quiescence_captures(&state, &weights, None, false, true, true, false);
        let gen_pathish = gen
            .iter()
            .filter(|m| quiesce_move_looks_path_or_multileg(&state, m))
            .count();
        let gen_pathish_non_promo = gen
            .iter()
            .filter(|m| {
                quiesce_move_looks_path_or_multileg(&state, m) && !is_loud_promotion_move(&state, m)
            })
            .count();
        eprintln!(
            "opening captures_only pathish={pathish} q_gen={} q_gen_pathish={gen_pathish} non_promo_pathish={gen_pathish_non_promo}",
            gen.len()
        );
        // Without a contested square, PathClear/MultiLeg must not enter q gen
        // (loud promotions into big pieces are the intentional exception).
        assert_eq!(gen_pathish_non_promo, 0);
        assert!(
            pathish > 0,
            "opening should have path-clear captures to filter"
        );
    }

    #[test]
    fn opening_worthwhile_quiescence_captures_far_fewer_than_raw() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let all = state.generate_legal_moves();
        let raw_caps = all
            .iter()
            .filter(|m| move_captures_enemy(&state, m))
            .count();
        let worth =
            generate_quiescence_captures(&state, &weights, None, false, true, true, false).len();
        let caps_only = state
            .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
            .len();
        eprintln!(
            "opening raw_captures={raw_caps} captures_only_gen={caps_only} worthwhile_q={worth}"
        );
        assert!(raw_caps > 0);
        assert!(
            worth < raw_caps,
            "50-point threshold should drop cheap opening jump-takes"
        );
    }

    #[test]
    fn free_king_promo_is_loud_and_enters_q_gen() {
        let weights = EvalWeights::seed();
        assert!(is_big_piece(PieceType::GreatGeneral));
        assert!(promotes_into_big_piece(PieceType::FreeKing));

        let mut state = GameState::new();
        // Black FreeKing already in the promotion zone (rank >= 25).
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        let fk = Position::new(17, 26).unwrap();
        state.place_piece(Piece::new(PieceType::FreeKing, Color::Black, fk));
        // Empty one-step forward landing.
        let to = Position::new(17, 27).unwrap();
        let promo = Move::new_with_promotion(fk, to, true);
        assert!(
            is_loud_promotion_move(&state, &promo),
            "FreeKing→GG should count as loud"
        );
        let gain = loud_promotion_material_gain(&state, &weights, &promo);
        assert!(
            gain >= min_quiescence_enemy_material(),
            "promo gain {gain} should clear loud floor"
        );
        let gen = generate_quiescence_captures(&state, &weights, None, false, false, true, false);
        assert!(
            gen.iter().any(|m| m.promoted && m.from == fk),
            "promo-only q gen must include FreeKing promotions: {gen:?}"
        );
    }

    #[test]
    fn quiescence_avoids_hanging_capture() {
        // After a hanging take, PathAware q expands the loud recapture of the hung
        // piece via the worthwhile floor (no AB→q dest-recapture seeding).
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::GoldGeneral, 1500.0);
        weights.piece.insert(PieceType::FreeKing, 2000.0);
        weights.piece.insert(PieceType::King, 100.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::FreeKing,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let hang = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        let _undo = state.make_move_for_search(hang).expect("hang take");
        // White to move: hung gold is a loud capture (≥ floor).
        let stand = evaluate_with_ply(&state, &weights, 0);
        let with_q = probe_quiescence(&state, &weights, 4, QPruneMode::PathAware, None);
        assert!(
            with_q.score > stand + 500,
            "q should take the hung gold: stand={stand} with_q={} q_caps={}",
            with_q.score,
            with_q.q_caps_searched
        );
    }

    #[test]
    fn ab_hang_prunes_high_value_guarded_capture() {
        // Seed GG (~4000) takes a pawn onto a guarded landing → skip in AB.
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        assert!(weights.piece_value(PieceType::GreatGeneral) >= HIGH_VALUE_HANGER);

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        // White gold at (10,15) attacks (10,14); does not attack (10,13).
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(10, 15).unwrap(),
        ));
        // Quiet alternative so search has a non-hanging move.
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(20, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let guarded = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 14).unwrap(),
        );
        let safe = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 13).unwrap(),
        );
        assert!(move_captures_enemy(&state, &guarded));
        assert!(move_captures_enemy(&state, &safe));

        let mut cache = LandingAttackCache::new();
        assert!(
            capture_hangs_high_value_piece(&state, &weights, &guarded, true, &mut cache),
            "guarded GG pawn-mop should hang-prune"
        );
        assert!(
            !capture_hangs_high_value_piece(&state, &weights, &safe, true, &mut cache),
            "safe GG landing past pawn should still be searchable"
        );

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert_ne!(
            result.best_move.as_ref().map(|m| (m.from, m.to)),
            Some((guarded.from, guarded.to)),
            "depth-1 must not pick the hanging GG capture"
        );
    }

    /// Slot 11 shape: Hook takes Gold then last royal (CP). Landing is attacked
    /// and net is ~14 vs Hook 6237 — hang-skip used to drop an instant win.
    fn hook_takes_gold_then_last_cp() -> (GameState, EvalWeights, Move) {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(18, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(4, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(16, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::CrownPrince,
            Color::Black,
            Position::new(18, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(19, 0).unwrap(),
        ));
        state.set_current_turn(Color::White);
        let mv = Move::new_two_step(
            Position::new(4, 0).unwrap(),
            Position::new(16, 0).unwrap(),
            Position::new(18, 0).unwrap(),
        );
        (state, weights, mv)
    }

    #[test]
    fn ab_hang_keeps_royal_capture_even_when_hook_hangs() {
        let (state, weights, take) = hook_takes_gold_then_last_cp();
        assert!(
            capture_takes_enemy_royal(&state, &take),
            "two-step via Gold onto CP must count as a royal take"
        );
        assert!(
            capture_takes_last_enemy_royal(&state, &take),
            "Black has only CP — this is an instant win"
        );
        let gold_only = Move::new(Position::new(4, 0).unwrap(), Position::new(16, 0).unwrap());
        assert!(!capture_takes_last_enemy_royal(&state, &gold_only));
        assert!(
            move_order_score_fresh(&state, &weights, &take)
                > move_order_score_fresh(&state, &weights, &gold_only),
            "last-royal must order above taking Gold alone"
        );
        assert!(move_captures_enemy(&state, &take));
        let mut cache = LandingAttackCache::new();
        assert!(
            !capture_hangs_high_value_piece(&state, &weights, &take, true, &mut cache),
            "must not hang-skip a last-royal Hook take"
        );

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("White must have a move");
        assert_eq!(
            (best.from, best.intermediate(), best.to),
            (take.from, take.intermediate(), take.to),
            "depth-1 must take the last royal, got {:?}",
            best
        );
        assert!(
            result.score >= 900_000,
            "last-royal take must score as mate, got {}",
            result.score
        );
        assert_eq!(
            result.root_moves_scored, 1,
            "root must stop after the instant win, scored {}",
            result.root_moves_scored
        );
    }

    /// Slot 0240 end: Hook on file 17 two-steps through empty 17,35 onto King 18,35.
    fn hook_two_step_mates_king() -> (GameState, EvalWeights, Move) {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(18, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(17, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::Black,
            Position::new(17, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        let mv = Move::new_two_step(
            Position::new(17, 11).unwrap(),
            Position::new(17, 35).unwrap(),
            Position::new(18, 35).unwrap(),
        );
        (state, weights, mv)
    }

    #[test]
    fn two_step_royal_take_is_one_search_ply() {
        let (mut state, _weights, take) = hook_two_step_mates_king();
        assert!(take.is_two_step());
        assert!(capture_takes_last_enemy_royal(&state, &take));
        let undo = state
            .make_move_for_search(take)
            .expect("two-step must apply in one make");
        assert_eq!(state.get_winner(), Some(Color::Black));
        assert!(state.has_lost(Color::White));
        state.unmake_move_for_search(undo);
        assert!(state.get_winner().is_none());
    }

    #[test]
    fn q_includes_hook_two_step_king_take_off_prev_to() {
        let (state, weights, take) = hook_two_step_mates_king();
        assert!(stm_has_royal_capture(&state));
        // Quiet parent landed somewhere else (slot0240 Peacock 21,32).
        let prev = Position::new(21, 32).unwrap();
        let gen =
            generate_quiescence_captures(&state, &weights, Some(prev), false, true, true, false);
        assert!(
            gen.iter()
                .any(|m| { m.from == take.from && m.to == take.to && m.is_two_step() }),
            "q must inject Hook×King two-step even when prev_to is not the king: {gen:?}"
        );
    }

    #[test]
    fn quiet_parent_q_sees_two_step_king_mate() {
        let (state, weights, _take) = hook_two_step_mates_king();
        let stand = evaluate_with_ply(&state, &weights, 0);
        let (score, q_nodes) = probe_quiet_parent_leaf_or_quiesce(&state, &weights, 2);
        assert!(
            q_nodes > 0,
            "quiet parent must open q when a royal take exists"
        );
        assert!(
            score >= 900_000,
            "Hook two-step onto king is one q ply and mate, stand={stand} q={score}"
        );
    }

    #[test]
    fn depth1_search_plays_two_step_king_mate() {
        let (state, weights, take) = hook_two_step_mates_king();
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 2,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("Black must have a move");
        assert_eq!(
            (best.from, best.intermediate(), best.to),
            (take.from, take.intermediate(), take.to),
            "depth-1 must play the two-step mate as one ply, got {:?}",
            best
        );
        assert!(
            result.score >= 900_000,
            "two-step king take must score as mate, got {}",
            result.score
        );
    }

    #[test]
    fn depth1_q_refuses_quiet_that_allows_hook_king_mate() {
        let (mut state, weights, _take) = hook_two_step_mates_king();
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(16, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::SilverGeneral,
            Color::White,
            Position::new(22, 33).unwrap(),
        ));
        // Occupying the other corner (18,11) does not help: the Hook captures
        // there on the first leg and still takes the King on file 18. Block the
        // *second* leg instead — 18,20 is not a first-dest from 17,11.
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(18, 20).unwrap(),
        ));
        state.set_current_turn(Color::White);
        let hang = Move::new(
            Position::new(22, 33).unwrap(),
            Position::new(21, 32).unwrap(),
        );
        let save = Move::new(
            Position::new(16, 35).unwrap(),
            Position::new(17, 34).unwrap(),
        );
        assert!(
            state
                .generate_legal_moves()
                .iter()
                .any(|m| same_root_move(m, &hang)),
            "Silver step must be legal"
        );
        assert!(
            state
                .generate_legal_moves()
                .iter()
                .any(|m| same_root_move(m, &save)),
            "Gold interpose on file 17 must be legal"
        );
        {
            let undo = state.make_move_for_search(save.clone()).expect("save");
            assert!(
                !stm_has_royal_capture(&state),
                "Gold on 17,34 must close both Hook paths to the King"
            );
            state.unmake_move_for_search(undo);
        }
        {
            let undo = state.make_move_for_search(hang.clone()).expect("hang");
            assert!(
                stm_has_royal_capture(&state),
                "Silver shuffle must leave Hook×King legal"
            );
            state.unmake_move_for_search(undo);
        }

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 2,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let hang_sc = result
            .root_lines
            .iter()
            .find(|(m, _)| same_root_move(m, &hang))
            .map(|(_, s)| *s);
        assert_eq!(
            hang_sc,
            Some(-1_000_000),
            "quiet that allows Hook×King must be mate after q, got {hang_sc:?}"
        );
        let best = result.best_move.expect("White must have a move");
        assert!(
            !same_root_move(&best, &hang),
            "depth-1 must not play the hanging Silver, got {:?}",
            best
        );
        assert!(
            result.score > -900_000,
            "chosen move must not be mate, got {}",
            result.score
        );
    }

    #[test]
    fn ab_hang_interior_keeps_pathclear_when_victim_was_sole_defender() {
        // GG path-clears a Hook Mover then lands past it. Pre-move the landing looks
        // attacked by the HM; post-fire it is safe. Interior confirm-on-prune must
        // not skip the take (the ply-71 false positive).
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        assert!(weights.piece_value(PieceType::GreatGeneral) >= HIGH_VALUE_HANGER);
        assert!(weights.piece_value(PieceType::HookMover) >= HIGH_VALUE_HANGER);

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        // Black GG on file 10; White HM between GG and a farther empty landing.
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 30).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::HookMover,
            Color::White,
            Position::new(10, 20).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let past_hm = Move::new(
            Position::new(10, 30).unwrap(),
            Position::new(10, 15).unwrap(),
        );
        assert!(
            move_captures_enemy(&state, &past_hm),
            "GG must path-clear the HM"
        );
        let (enemy, _own, kind) = capture_exchange_kind(&state, &weights, &past_hm);
        assert_eq!(kind, CaptureKind::PathClear);
        assert!(enemy >= weights.piece_value(PieceType::HookMover) - 1.0);

        let board = state.get_board();
        assert!(
            board.is_position_attacked_by_color(past_hm.to, Color::White),
            "pre-move: HM attacks the landing"
        );
        let gg = board.get_piece(past_hm.from).unwrap();
        let vb = crate::move_simulation::simulate_move(board, &past_hm, &gg);
        assert!(
            !vb.is_position_attacked_by_color(past_hm.to, Color::White),
            "post-fire: landing safe once HM is gone"
        );

        let mut cache = LandingAttackCache::new();
        assert!(
            !capture_hangs_high_value_piece(&state, &weights, &past_hm, false, &mut cache),
            "interior hang prune must not skip path-clear past a defending victim"
        );
        assert!(
            !capture_hangs_high_value_piece(&state, &weights, &past_hm, true, &mut cache),
            "root post-fire hang prune must also keep the safe path-clear"
        );

        // Depth-2 from White-to-move after hanging the HM on this file should prefer
        // not leaving it there — at least score the take in Black's reply.
        // Flip: White just moved HM onto the file; Black to move is covered above.
        // From Black's seat at d1, the take should be chosen.
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("GG should have a move");
        assert_eq!(best.from, past_hm.from);
        assert!(
            move_captures_enemy(&state, &best),
            "depth-1 should take the hung HM via path-clear, got {:?}→{:?}",
            best.from,
            best.to
        );
    }

    #[test]
    fn ab_hang_keeps_safe_high_victim_capture() {
        // Low-value mover takes unprotected GG — still searchable.
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let take_gg = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        assert!(move_captures_enemy(&state, &take_gg));
        let mut cache = LandingAttackCache::new();
        assert!(
            !capture_hangs_high_value_piece(&state, &weights, &take_gg, true, &mut cache),
            "taking unprotected GG with gold must not hang-prune"
        );

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert_eq!(
            result.best_move.as_ref().map(|m| m.to),
            Some(Position::new(10, 11).unwrap()),
            "depth-1 should still take the free Great General"
        );

        let with_q = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 2,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert!(
            with_q.q_nodes > 0,
            "loud capture AB leaf should enter quiescence"
        );
    }

    #[test]
    fn quiet_ab_leaf_skips_quiescence() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        // Black pawn can only push (quiet); no captures available.
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 4,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert_eq!(
            result.q_nodes, 0,
            "quiet AB leaves must not enter quiescence"
        );
        assert!(result.best_move.is_some());
    }

    #[test]
    fn cheap_capture_ab_leaf_skips_quiescence() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();
        assert!(weights.piece_value(PieceType::Pawn) < min_quiescence_enemy_material());

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let take_pawn = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        assert!(move_captures_enemy(&state, &take_pawn));
        assert!(
            capture_material_exchange(&state, &weights, &take_pawn).0
                < min_quiescence_enemy_material()
        );

        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 4,
                q_prune_mode: QPruneMode::PathAware,
                ..Default::default()
            },
        );
        assert_eq!(
            result.q_nodes, 0,
            "below-floor capture leaf must not enter quiescence"
        );
    }

    #[test]
    fn see_orders_safe_landing_above_guarded() {
        // PathClear post-fire hang: guarded landing demoted when net < frac*mover.
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::GreatGeneral, 90.0);
        weights.piece.insert(PieceType::Pawn, 1.0);
        weights.piece.insert(PieceType::GoldGeneral, 50.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        // Capturing-range GG can take the white pawn by landing past it.
        state.place_piece(Piece::new(
            PieceType::GreatGeneral,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        // Guards only (10, 14): one step south of gold at (10, 15). Does not attack (10, 13).
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::White,
            Position::new(10, 15).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let safe = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 13).unwrap(),
        );
        let guarded = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 14).unwrap(),
        );
        assert!(move_captures_enemy(&state, &safe));
        assert!(move_captures_enemy(&state, &guarded));
        let safe_s = move_order_score_fresh(&state, &weights, &safe);
        let guarded_s = move_order_score_fresh(&state, &weights, &guarded);
        assert!(
            safe_s > guarded_s,
            "safe landing {safe_s} should outrank guarded {guarded_s}"
        );
    }

    #[test]
    fn see_lva_prefers_cheaper_attacker_equal_gain() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::Pawn, 1.0);
        weights.piece.insert(PieceType::GoldGeneral, 50.0);
        weights.rebuild_piece_value_table();

        let mut state = GameState::new();
        state.place_piece(Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(35, 35).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(10, 11).unwrap(),
        ));
        // Black pawn captures forward onto the white pawn (unguarded).
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::Black,
            Position::new(10, 10).unwrap(),
        ));
        // Black gold also attacks the same square.
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(11, 11).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let by_pawn = Move::new(
            Position::new(10, 10).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        let by_gold = Move::new(
            Position::new(11, 11).unwrap(),
            Position::new(10, 11).unwrap(),
        );
        let pawn_s = move_order_score_fresh(&state, &weights, &by_pawn);
        let gold_s = move_order_score_fresh(&state, &weights, &by_gold);
        assert!(
            pawn_s > gold_s,
            "cheaper attacker should rank higher: pawn={pawn_s} gold={gold_s}"
        );
    }

    fn find_labeled_move(state: &GameState, from: (u8, u8), to: (u8, u8)) -> Move {
        // Labels are 1-based; Position is 0-based.
        let from = Position::new(from.0 - 1, from.1 - 1).unwrap();
        let to = Position::new(to.0 - 1, to.1 - 1).unwrap();
        state
            .generate_legal_moves()
            .into_iter()
            .find(|m| m.from == from && m.to == to)
            .unwrap_or_else(|| panic!("missing move {:?}->{:?}", from, to))
    }

    fn apply_gg(state: &mut GameState, from: (u8, u8), to: (u8, u8)) {
        let mv = find_labeled_move(state, from, to);
        // Search make applies capturing-range GG; SearchUndo has no Drop unmake.
        let _undo = state
            .make_move_for_search(mv.clone())
            .unwrap_or_else(|| panic!("GG apply failed {}", move_label(state, &mv)));
    }

    /// Release harness: compare q-prune modes on opening + post-GG leaves.
    /// Run: `cargo test -r --lib qprune_mode_matrix -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn qprune_mode_matrix() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.rebuild_piece_value_table();

        let modes = [
            ("baseline", QPruneMode::Baseline),
            ("A_net_gain", QPruneMode::NetGainDelta),
            ("B_top_n", QPruneMode::TopN),
            ("C_recapture", QPruneMode::RecaptureOnly),
            ("D_stale_hang", QPruneMode::StaleHang),
            ("A+B", QPruneMode::NetGainAndTopN),
            ("B+C", QPruneMode::TopNAndRecapture),
            ("PathAware", QPruneMode::PathAware),
        ];

        let mut opening = GameState::new();
        opening.setup_initial_position();

        // Logged blowups: black-side GG 18,4→18,26 and white-side GG 19,33→19,11.
        let mut after_gg_black = opening.clone();
        apply_gg(&mut after_gg_black, (18, 4), (18, 26));

        let mut after_gg_white = opening.clone();
        apply_gg(&mut after_gg_white, (18, 4), (18, 26));
        apply_gg(&mut after_gg_white, (19, 33), (19, 11));

        let positions = [
            ("opening", opening.clone()),
            ("after_GG_18_4", after_gg_black),
            ("after_GG_19_33", after_gg_white),
        ];

        eprintln!(
            "{:<14} {:<12} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8}",
            "pos", "mode", "ms", "nodes", "qnodes", "caps_gen", "caps_srch", "score"
        );

        for (pos_name, pos) in &positions {
            for (mode_name, mode) in &modes {
                let t0 = Instant::now();
                // Leaf qsearch open-window: isolates prune effect from AB.
                let r = probe_quiescence(pos, &weights, 6, *mode, Some(15_000));
                let ms = t0.elapsed().as_millis();
                eprintln!(
                    "{:<14} {:<12} {:>10} {:>10} {:>10} {:>10} {:>10} {:>8}",
                    pos_name,
                    mode_name,
                    ms,
                    r.nodes,
                    r.q_nodes,
                    r.q_caps_generated,
                    r.q_caps_searched,
                    r.score
                );
            }
        }

        // Full AB smoke: opening d3/q6 with 8s ceiling per mode (wall-clock relevance).
        eprintln!("\n--- full AB d=3 q=6 max 8s (opening) ---");
        for (mode_name, mode) in &modes {
            let t0 = Instant::now();
            let r = search(
                &opening,
                &weights,
                &SearchConfig {
                    depth: 3,
                    max_time_ms: Some(8_000),
                    collect_trace: false,
                    quiescence_depth: 6,
                    q_prune_mode: *mode,
                    ..Default::default()
                },
            );
            let ms = t0.elapsed().as_millis();
            let best = r
                .best_move
                .as_ref()
                .map(|m| move_label(&opening, m))
                .unwrap_or_else(|| "-".into());
            eprintln!(
                "{:<12} ms={ms} nodes={} qnodes={} score={} best={}",
                mode_name, r.nodes, r.q_nodes, r.score, best
            );
        }
    }

    #[test]
    fn search_prefers_capture_over_repeating_quiet() {
        let mut state = GameState::new();
        state.clear_board();
        let b_from = Position::new(10, 10).unwrap();
        let b_to = Position::new(10, 11).unwrap();
        let capture_to = Position::new(11, 10).unwrap();
        let w_from = Position::new(20, 20).unwrap();
        let w_to = Position::new(20, 21).unwrap();
        state.place_piece(Piece::new(PieceType::King, Color::Black, b_from));
        state.place_piece(Piece::new(PieceType::King, Color::White, w_from));
        state.place_piece(Piece::new(PieceType::Pawn, Color::White, capture_to));
        state.set_current_turn(Color::Black);
        state.reset_rep_history();

        // One full cycle so returning to the quiet intermediate is a 2nd visit.
        let _ = state.make_move(Move::new(b_from, b_to));
        let _ = state.make_move(Move::new(w_from, w_to));
        let _ = state.make_move(Move::new(b_to, b_from));
        let _ = state.make_move(Move::new(w_to, w_from));
        assert_eq!(state.get_current_turn(), Color::Black);
        assert!(state.repetition_count() >= 2);

        let weights = EvalWeights::seed();
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::Baseline,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("move");
        assert_eq!(
            best.to, capture_to,
            "expected capture {:?}, got {:?}",
            capture_to, best
        );
        // Looping quiet should score as draw from the child node.
        let loop_line = result
            .root_lines
            .iter()
            .find(|(m, _)| m.to == b_to)
            .expect("loop candidate");
        assert_eq!(loop_line.1, 0);
    }

    #[test]
    fn search_prefers_progress_reset_over_walking_into_draw() {
        use crate::game_state::PROGRESS_DRAW_LIMIT;

        let mut state = GameState::new();
        state.clear_board();
        let b_from = Position::new(10, 10).unwrap();
        let capture_to = Position::new(11, 10).unwrap();
        let quiet_to = Position::new(10, 11).unwrap();
        let w_king = Position::new(20, 20).unwrap();
        state.place_piece(Piece::new(PieceType::King, Color::Black, b_from));
        state.place_piece(Piece::new(PieceType::King, Color::White, w_king));
        state.place_piece(Piece::new(PieceType::Pawn, Color::White, capture_to));
        // Extra Gold so a clock reset stays ahead; two kings alone eval to ~0.
        // Mid-board: not in a promotion zone (a Gold promo would also reset).
        state.place_piece(Piece::new(
            PieceType::GoldGeneral,
            Color::Black,
            Position::new(18, 18).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        state.set_turns_without_capture_or_promotion(PROGRESS_DRAW_LIMIT - 1);
        state.reset_rep_history();

        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::Baseline,
                ..Default::default()
            },
        );
        let best = result.best_move.expect("move");
        assert_eq!(
            best.to, capture_to,
            "expected pawn take to reset the clock, got {:?}",
            best
        );
        let quiet = result
            .root_lines
            .iter()
            .find(|(m, _)| m.to == quiet_to)
            .expect("quiet king step");
        assert_eq!(
            quiet.1, 0,
            "one more quiet should walk into the 100-move draw"
        );
        assert!(result.score > 0, "resetting capture must beat the draw");
    }
}
