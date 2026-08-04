//! Alpha-beta search over GameState with make/unmake, compact traces for the GUI.

use crate::eval::{evaluate_with_ply, seed_loud_capture_floor, EvalWeights};
use crate::game_state::{GameState, LegalMoveGen, Move};
use crate::movement::{BlockingMode, MovementCapability, MovementConfig};
use crate::move_simulation::BoardLike;
use crate::path_utils;
use crate::piece::Color;
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
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
    /// Quiescence runs only when this is ≥ the loud-capture floor.
    last_ab_capture_enemy: f32,
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
}

impl TranspositionTable {
    fn new(size_pow2: usize) -> Self {
        let n = size_pow2.next_power_of_two().max(1024);
        Self {
            entries: vec![None; n],
        }
    }

    fn index(&self, key: u64) -> usize {
        (key as usize) & (self.entries.len() - 1)
    }

    fn probe(&self, key: u64) -> Option<&TtEntry> {
        let e = self.entries[self.index(key)].as_ref()?;
        if e.key == key {
            Some(e)
        } else {
            None
        }
    }

    fn store(&mut self, entry: TtEntry) {
        let i = self.index(entry.key);
        let replace = match &self.entries[i] {
            None => true,
            Some(old) => entry.depth >= old.depth || old.key != entry.key,
        };
        if replace {
            self.entries[i] = Some(entry);
        }
    }
}

fn position_hash(state: &GameState) -> u64 {
    state.hash()
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

    if board
        .get_piece(mv.to)
        .is_some_and(|p| p.color == enemy)
    {
        return true;
    }
    if let Some(inter) = mv.intermediate() {
        if board
            .get_piece(inter)
            .is_some_and(|p| p.color == enemy)
        {
            return true;
        }
    }
    if let Some(path) = mv.free_eagle_path() {
        return path.iter().skip(1).any(|pos| {
            board
                .get_piece(*pos)
                .is_some_and(|p| p.color == enemy)
        });
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
                && board
                    .get_piece(pos)
                    .is_some_and(|p| p.color == enemy)
            {
                return true;
            }
        }
    }
    false
}

/// Enemy material taken vs own material destroyed by the move itself
/// (capturing-range / FE path clears). Does not model recapture of the mover.
fn capture_material_exchange(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
) -> (f32, f32) {
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
            let v = weights.piece_value(p.piece_type);
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
            if pos != mv.from && pos != mv.to {
                if board.get_piece(pos).is_some() {
                    path_occupied = true;
                    add(pos);
                }
            }
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
            let v = weights.piece_value(p.piece_type);
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

/// True when a capture hangs a high-value mover and should be skipped in AB.
///
/// Conditions: mover value ≥ [`HIGH_VALUE_HANGER`], net below [`HANG_NET_FRAC`] of
/// mover, and landing attacked. PathClear/MultiLeg use post-fire attack when
/// `postfire_pathclear_hang` (root); interior uses the cheap pre-move check.
fn capture_hangs_high_value_piece(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    postfire_pathclear_hang: bool,
    attack_cache: &mut HashMap<usize, bool>,
) -> bool {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return false;
    };
    let mover_value = weights.piece_value(mover.piece_type);
    if mover_value < HIGH_VALUE_HANGER {
        return false;
    }
    let (enemy, own, kind) = capture_exchange_kind(state, weights, mv);
    if enemy == 0.0 || !net_below_hang_frac(enemy, own, mover_value) {
        return false;
    }
    let opponent = state.get_current_turn().opposite();
    match kind {
        CaptureKind::PathClear | CaptureKind::MultiLeg if postfire_pathclear_hang => {
            let vb = crate::move_simulation::simulate_move(board, mv, &mover);
            vb.is_position_attacked_by_color(mv.to, opponent)
        }
        CaptureKind::PathClear | CaptureKind::MultiLeg | CaptureKind::SimpleTake => {
            landing_attacked_cached(board, mv.to, opponent, attack_cache)
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
    let (enemy, _own) = capture_material_exchange(state, weights, mv);
    enemy >= min_quiescence_enemy_material()
}

/// MVV-LVA capture score without hang checks (for quiescence ordering).
fn mvv_lva_score(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    let mover_value = weights.piece_value(mover.piece_type);
    let (enemy, own) = capture_material_exchange(state, weights, mv);
    if enemy == 0.0 {
        return i32::MIN / 4;
    }
    ((enemy - own) * 1000.0 - mover_value).round() as i32
}

/// Move-ordering score (heuristic only — not search correctness).
///
/// Captures: `gain = enemy - own`. SimpleTake uses a cheap pre-move landing
/// attack cache. When `postfire_pathclear_hang` is set, PathClear / MultiLeg
/// use post-fire simulation if net gain is below [`HANG_NET_FRAC`] of mover
/// (root ordering); interior keeps the cheap pre-move check. LVA: `gain*1000 - mover`.
fn move_order_score(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    opponent: Color,
    attack_cache: &mut HashMap<usize, bool>,
    postfire_pathclear_hang: bool,
) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    let mover_value = weights.piece_value(mover.piece_type);
    let (enemy, own, kind) = capture_exchange_kind(state, weights, mv);
    if enemy == 0.0 {
        return i32::MIN / 4;
    }

    let mut gain = enemy - own;
    let hanging = if net_below_hang_frac(enemy, own, mover_value) {
        match kind {
            CaptureKind::PathClear | CaptureKind::MultiLeg if postfire_pathclear_hang => {
                let vb = crate::move_simulation::simulate_move(board, mv, &mover);
                vb.is_position_attacked_by_color(mv.to, opponent)
            }
            CaptureKind::PathClear | CaptureKind::MultiLeg | CaptureKind::SimpleTake => {
                landing_attacked_cached(board, mv.to, opponent, attack_cache)
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

fn landing_attacked_cached(
    board: &crate::board::Board,
    to: Position,
    opponent: Color,
    cache: &mut HashMap<usize, bool>,
) -> bool {
    let idx = to.to_index();
    if let Some(&hit) = cache.get(&idx) {
        return hit;
    }
    let hit = board.is_position_attacked_by_color(to, opponent);
    cache.insert(idx, hit);
    hit
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
    if !uses_capturing {
        return false;
    }
    path_utils::get_path_positions(mv.from, mv.to)
        .into_iter()
        .any(|p| p != mv.from && p != mv.to && board.get_piece(p).is_some())
}

/// Captures worth expanding in quiescence.
///
/// Contract: q finishes the contested square (loud SimpleTakes + recaptures).
/// Capturing-range corridor wipes / multi-leg snipes belong to main search unless
/// they are a destination recapture onto `prev_to`.
///
/// Uses capture-oriented generation (no quiet ray fan-out / quiet multi-leg).
/// Includes last-move recaptures even below the loud floor.
fn generate_quiescence_captures(
    state: &GameState,
    weights: &EvalWeights,
    prev_to: Option<Position>,
) -> Vec<Move> {
    state
        .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
        .into_iter()
        .filter(|mv| {
            if !is_quiescence_capture_candidate(state, weights, mv, prev_to) {
                return false;
            }
            // Same policy as PathAware keep: PathClear/MultiLeg only as dest recapture.
            if quiesce_move_looks_path_or_multileg(state, mv) && prev_to != Some(mv.to) {
                return false;
            }
            true
        })
        .collect()
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
        tt: TranspositionTable::new(1 << 20),
        q_tt: TranspositionTable::new(1 << 18),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        last_ab_capture_enemy: 0.0,
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
            if is_capture {
                let mut hang_cache = HashMap::new();
                if capture_hangs_high_value_piece(state, weights, mv, true, &mut hang_cache) {
                    continue;
                }
            }
            let child_depth = d - 1;
            // Root LMR: late quiets at ID depth >= 2 (pre–PR17 rule).
            let can_reduce = d >= 2 && i >= 3 && !is_capture && child_depth >= 1;
            let reduction = if can_reduce {
                (if i >= 12 { 2 } else { 1 }).min(child_depth)
            } else {
                0
            };

            let Some(undo) = pos.make_move_for_search(mv.clone()) else {
                continue;
            };
            ctx.last_ab_capture_enemy = if is_capture {
                capture_material_exchange(state, weights, mv).0
            } else {
                0.0
            };
            ctx.nodes += 1;
            ctx.ply = root_ply + 1;
            ctx.phase = "search";
            ctx.q_label.clear();
            ctx.q_caps_at_node = 0;
            ctx.q_cap_index = 0;

            // Root PVS: first move full window (PV); later moves null-window then
            // full-window research on fail-high (research keeps full q depth).
            let mut score = if i == 0 {
                if reduction > 0 {
                    let reduced = child_depth - reduction;
                    -alphabeta(&mut pos, weights, reduced, -beta, -alpha, true, &mut ctx)
                } else {
                    -alphabeta(&mut pos, weights, child_depth, -beta, -alpha, true, &mut ctx)
                }
            } else {
                ctx.root_pvs_tried += 1;
                let nw_beta = alpha.saturating_add(1);
                let mut s = if reduction > 0 {
                    let reduced = child_depth - reduction;
                    -alphabeta(&mut pos, weights, reduced, -nw_beta, -alpha, false, &mut ctx)
                } else {
                    -alphabeta(&mut pos, weights, child_depth, -nw_beta, -alpha, false, &mut ctx)
                };
                if !ctx.abort && s > alpha {
                    ctx.root_fail_high += 1;
                    s = -alphabeta(&mut pos, weights, child_depth, -beta, -alpha, true, &mut ctx);
                }
                s
            };
            if i == 0 && reduction > 0 && !ctx.abort && score > alpha {
                score = -alphabeta(&mut pos, weights, child_depth, -beta, -alpha, true, &mut ctx);
            }

            pos.unmake_move_for_search(undo);
            ctx.ply = root_ply;
            ctx.q_nodes_last_root = ctx.q_nodes.saturating_sub(ctx.q_nodes_at_root_start);

            if ctx.abort {
                finished_iteration = false;
                break;
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
            "ab diag: root={} fh={}% near20={}% spread(max-med)={} qnodes={} quniq={}{} qTThit={}%/{}/{} abort={}",
            ctx.root_total,
            fh_pct,
            near_pct,
            spread,
            ctx.q_nodes,
            quniq,
            if ctx.q_unique_saturated { "+" } else { "" },
            qhit_pct,
            ctx.q_tt_hits,
            ctx.q_tt_probes,
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
    };
    let mut pos = state.clone();
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
            false,
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
    }
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
                ctx.last_ab_capture_enemy = if move_captures_enemy(state, best_move) {
                    capture_material_exchange(state, weights, best_move).0
                } else {
                    0.0
                };
                ctx.ply = root_ply + 1;
                ctx.phase = "trace";
                let (_score, subtree) =
                    alphabeta_record(&mut child, weights, depth - 1, &mut *ctx);
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
    const MATE_BAND: i32 = 900_000;
    if ctx.allow_null && depth >= 2 && beta < MATE_BAND && beta > -MATE_BAND {
        let r = NULL_R.min(depth - 1);
        ctx.allow_null = false;
        let prev_turn = state.get_current_turn();
        let saved_enemy = ctx.last_ab_capture_enemy;
        ctx.last_ab_capture_enemy = 0.0;
        state.set_current_turn(prev_turn.opposite());
        let parent_ply = ctx.ply;
        ctx.ply = parent_ply + 1;
        let null_depth = depth - 1 - r;
        let score = if null_depth == 0 {
            -evaluate_with_ply(state, weights, ctx.ply)
        } else {
            -alphabeta(state, weights, null_depth, -beta, -beta + 1, false, ctx)
        };
        ctx.ply = parent_ply;
        state.set_current_turn(prev_turn);
        ctx.last_ab_capture_enemy = saved_enemy;
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
        state,
        weights,
        depth,
        alpha,
        beta,
        is_pv,
        ctx,
        parent_ply,
        moves,
        0,
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
                stage_b,
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
    moves: Vec<Move>,
    move_index_base: usize,
) -> (i32, Option<MoveKey>, i32, bool) {
    let mut best = i32::MIN + 1;
    let mut best_move_key = None;
    let mut did_cutoff = false;
    let mut hang_cache = HashMap::new();

    for (i, mv) in moves.into_iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        let mv_key = move_tt_key(&mv);
        let is_capture = move_captures_enemy(state, &mv);
        if is_capture
            && capture_hangs_high_value_piece(state, weights, &mv, false, &mut hang_cache)
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
            && !is_killer;
        let reduction = if can_reduce {
            if move_index >= 12 {
                2
            } else {
                LMR_R
            }
            .min(depth - 1)
        } else {
            0
        };

        let capture_enemy = if is_capture {
            capture_material_exchange(state, weights, &mv).0
        } else {
            0.0
        };
        let Some(undo) = state.make_move_for_search(mv.clone()) else {
            continue;
        };
        ctx.last_ab_capture_enemy = capture_enemy;
        ctx.ply = parent_ply + 1;

        // PV only along the first move of a PV node (root PVS / research sets
        // is_pv). Non-PV leaves use capped quiescence when config > 2.
        let child_pv = is_pv && i == 0 && reduction == 0;

        let mut score = if reduction > 0 {
            let reduced = depth - 1 - reduction;
            -alphabeta(state, weights, reduced, -beta, -alpha, false, ctx)
        } else {
            -alphabeta(state, weights, depth - 1, -beta, -alpha, child_pv, ctx)
        };

        // Re-search at full depth if reduced search looks interesting.
        // Fail-high research restores PV (full q) when the parent was PV.
        if reduction > 0 && !ctx.abort && score > alpha {
            score = -alphabeta(state, weights, depth - 1, -beta, -alpha, is_pv && i == 0, ctx);
        }

        state.unmake_move_for_search(undo);
        ctx.ply = parent_ply;

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
    // Q only after loud AB captures; quiet / below-floor takes use stand-pat.
    if ctx.last_ab_capture_enemy < min_quiescence_enemy_material() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }
    let q = leaf_quiescence_depth(ctx, is_pv);
    if q == 0 {
        evaluate_with_ply(state, weights, ctx.ply)
    } else {
        ctx.phase = "quiesce";
        ctx.quiesce_entry_depth = q;
        // Search skips move_history; only history-backed positions seed prev_to.
        // Dest-recapture of below-floor victims is left to AB depth / worthwhile floor.
        let prev_to = state.get_move_history().last().map(|m| m.to);
        quiesce(state, weights, q, alpha, beta, prev_to, false, ctx)
    }
}

/// Capture-only quiescence (excludes pure self-captures via `move_captures_enemy`).
///
/// Q contract: resolve hanging exchanges on the contested square (SimpleTakes /
/// dest-recaptures). Non-recapture PathClear/MultiLeg corridor tactics are left
/// to main-search depth (pre–PR 17-style behavior under high piece values).
///
/// `prev_to` / `prev_was_simple`: prior move landing and whether it was a
/// [`CaptureKind::SimpleTake`] (PathAware recapture exception / RecaptureOnly).
fn quiesce(
    state: &mut GameState,
    weights: &EvalWeights,
    qdepth: u32,
    mut alpha: i32,
    beta: i32,
    prev_to: Option<Position>,
    _prev_was_simple: bool,
    ctx: &mut SearchContext,
) -> i32 {
    ctx.nodes += 1;
    ctx.q_nodes += 1;
    ctx.q_depth_left = qdepth;

    if state.get_winner().is_some() || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }

    let key = position_hash(state);
    if !ctx.q_unique_saturated {
        if ctx.q_unique.len() < Q_UNIQUE_CAP {
            ctx.q_unique.insert(key);
        } else {
            ctx.q_unique_saturated = true;
        }
    }

    // Quiescence TT: depth is remaining q-plies.
    ctx.q_tt_probes += 1;
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
    }
    let alpha_orig = alpha;

    let stand_pat = evaluate_with_ply(state, weights, ctx.ply);
    ctx.q_stand_pat = stand_pat;
    if qdepth == 0 {
        return stand_pat;
    }
    if stand_pat >= beta {
        ctx.q_tt.store(TtEntry {
            key,
            depth: qdepth,
            score: stand_pat,
            bound: TtBound::Lower,
            best: None,
        });
        return stand_pat;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let raw_moves = generate_quiescence_captures(state, weights, prev_to);
    if raw_moves.is_empty() {
        return stand_pat;
    }

    struct QCand {
        mv: Move,
        enemy: f32,
        own: f32,
        kind: CaptureKind,
        mover_value: f32,
        landing_victim: f32,
        is_recapture: bool,
        is_dest_recapture: bool,
    }

    let mut cands: Vec<QCand> = raw_moves
        .into_iter()
        .filter_map(|mv| {
            let mover_value = state
                .get_board()
                .get_piece(mv.from)
                .map(|p| weights.piece_value(p.piece_type))
                .unwrap_or(0.0);
            let landing_victim = state
                .get_board()
                .get_piece(mv.to)
                .filter(|p| p.color != state.get_current_turn())
                .map(|p| weights.piece_value(p.piece_type))
                .unwrap_or(0.0);
            let is_recapture = prev_to
                .map(|sq| capture_hits_square(state, &mv, sq))
                .unwrap_or(false);
            let is_dest_recapture = prev_to == Some(mv.to);
            let (enemy, own, kind) = capture_exchange_kind(state, weights, &mv);
            Some(QCand {
                mv,
                enemy,
                own,
                kind,
                mover_value,
                landing_victim,
                is_recapture,
                is_dest_recapture,
            })
        })
        .collect();

    let path_aware = ctx.q_prune_mode.uses_path_aware();
    let deep_ply = qdepth < ctx.quiesce_entry_depth;

    // Recapture-only after the first q-ply.
    if ctx.q_prune_mode.uses_recapture_only() {
        if prev_to.is_some() {
            cands.retain(|c| c.is_recapture);
            if cands.is_empty() {
                return stand_pat;
            }
        }
    }

    // PathAware deep taper: loud victims, or recapture onto the previous landing.
    if path_aware && deep_ply {
        let floor = min_quiescence_deep_enemy();
        cands.retain(|c| c.enemy >= floor || c.is_recapture);
        if cands.is_empty() {
            return stand_pat;
        }
    }

    let use_net = ctx.q_prune_mode.uses_net_gain();

    // Stale hang prune (pre-move landing attack).
    if ctx.q_prune_mode.uses_stale_hang() {
        let opponent = state.get_current_turn().opposite();
        let mut attack_cache: HashMap<usize, bool> = HashMap::new();
        let board = state.get_board();
        cands.retain(|c| {
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
        .map(|c| if use_net { c.enemy - c.own } else { c.enemy })
        .fold(0.0f32, f32::max);
    if stand_pat.saturating_add(best_gain.round() as i32) <= alpha {
        return stand_pat;
    }

    // Landing victim first, soft-boost recaptures, then path-sum MVV-LVA.
    cands.sort_by(|a, b| {
        let la = (a.landing_victim * 1000.0).round() as i32;
        let lb = (b.landing_victim * 1000.0).round() as i32;
        lb.cmp(&la)
            .then_with(|| b.is_recapture.cmp(&a.is_recapture))
            .then_with(|| {
                let sa = ((a.enemy - a.own) * 1000.0 - a.mover_value).round() as i32;
                let sb = ((b.enemy - b.own) * 1000.0 - b.mover_value).round() as i32;
                sb.cmp(&sa)
            })
    });

    ctx.q_caps_generated = ctx
        .q_caps_generated
        .saturating_add(cands.len() as u64);

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
    // PathAware: PathClear/MultiLeg only as destination recapture; deep plies
    // also restrict SimpleTakes to recaptures onto the previous landing.
    if path_aware {
        let mut kept = Vec::with_capacity(top_n_cap.min(cands.len()));
        let mut path_kept = 0usize;
        for c in cands.drain(..) {
            if kept.len() >= top_n_cap {
                break;
            }
            match c.kind {
                CaptureKind::SimpleTake => {
                    if deep_ply && !c.is_recapture {
                        continue;
                    }
                }
                CaptureKind::PathClear | CaptureKind::MultiLeg => {
                    if path_kept >= QUIESCE_PATHCLEAR_DEEP_BUDGET {
                        continue;
                    }
                    if !pathclear_allowed_in_pathaware_q(c.is_dest_recapture) {
                        continue;
                    }
                    path_kept += 1;
                }
            }
            kept.push(c);
        }
        cands = kept;
    } else if top_n_cap != usize::MAX && cands.len() > top_n_cap {
        cands.truncate(top_n_cap);
    }

    let n_caps = cands.len();
    ctx.q_caps_at_node = n_caps;

    let mut best = stand_pat;
    let parent_ply = ctx.ply;
    let opponent = state.get_current_turn().opposite();

    for (i, c) in cands.into_iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        // Live delta: skip once earlier MVV takes have raised alpha.
        let gain = if use_net { c.enemy - c.own } else { c.enemy };
        if (stand_pat as f32 + gain) <= alpha as f32 {
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
        let mv = c.mv;

        // Pre-make hang skip for PathClear/MultiLeg: avoid expensive ray makes when
        // net is poor and the landing already looks attacked on the current board.
        if path_aware
            && matches!(kind, CaptureKind::PathClear | CaptureKind::MultiLeg)
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

        // PathAware: drop clearly hanging captures using the post-fire board
        // (PathClear can remove landing defenders / own cover).
        // Net gain vs mover*HANG_NET_FRAC so multi-piece PathClears still hang-check.
        if path_aware
            && matches!(
                kind,
                CaptureKind::SimpleTake | CaptureKind::PathClear | CaptureKind::MultiLeg
            )
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
        let next_was_simple = kind == CaptureKind::SimpleTake;

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
            next_was_simple,
            ctx,
        );
        state.unmake_move_for_search(undo);
        ctx.ply = parent_ply;
        ctx.q_depth_left = qdepth;
        ctx.q_caps_at_node = n_caps;

        if score > best {
            best = score;
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
        best: None,
    });
    best
}

/// True if this capture takes an enemy on `sq` (landing, intermediate, or path).
fn capture_hits_square(state: &GameState, mv: &Move, sq: Position) -> bool {
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
        let score = leaf_or_quiesce(
            state,
            weights,
            i32::MIN + 1,
            i32::MAX - 1,
            true,
            ctx,
        );
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
        let capture_enemy = if is_capture {
            capture_material_exchange(state, weights, &mv).0
        } else {
            0.0
        };
        let Some(undo) = state.make_move_for_search(mv) else {
            continue;
        };
        ctx.last_ab_capture_enemy = capture_enemy;
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
        b.best
            .cmp(&a.best)
            .then(b.score.unwrap_or(i32::MIN).cmp(&a.score.unwrap_or(i32::MIN)))
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
        let rms = now
            .duration_since(self.root_move_started)
            .as_millis();
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
    let mut attack_cache: HashMap<usize, bool> = HashMap::new();
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
/// `postfire_pathclear_hang`: use simulate_move for PathClear hang (root only —
/// too expensive on every interior node).
fn order_moves_with_heuristics(
    state: &GameState,
    weights: &EvalWeights,
    moves: &mut [Move],
    ctx: &SearchContext,
    ply: usize,
    captures_only_style: bool,
    postfire_pathclear_hang: bool,
) {
    let opponent = state.get_current_turn().opposite();
    let mut attack_cache: HashMap<usize, bool> = HashMap::new();
    moves.sort_by_key(|mv| {
        let cap = if captures_only_style {
            mvv_lva_score(state, weights, mv)
        } else {
            move_order_score(
                state,
                weights,
                mv,
                opponent,
                &mut attack_cache,
                postfire_pathclear_hang,
            )
        };
        let kr = killer_rank(ctx, ply, mv);
        let hist = history_score(ctx, mv);
        std::cmp::Reverse((cap, kr, hist))
    });
}

/// Test/helper: ordering score with a fresh per-call attack cache.
#[cfg(test)]
fn move_order_score_fresh(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let opponent = state.get_current_turn().opposite();
    let mut cache = HashMap::new();
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
            },
        );
        let best = result.best_move.expect("expected a move");
        assert_eq!(best.to, Position::new(10, 11).unwrap());
        assert!(result.score > 100_000, "mate-ish score, got {}", result.score);
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
        };
        let mut cfg_trace = cfg_play.clone();
        cfg_trace.collect_trace = true;
        let play = search(&state, &weights, &cfg_play);
        let traced = search(&state, &weights, &cfg_trace);
        assert_eq!(play.best_move.as_ref().map(|m| (m.from, m.to, m.promoted)), traced.best_move.as_ref().map(|m| (m.from, m.to, m.promoted)));
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
        let gen_none = generate_quiescence_captures(&state, &weights, None);
        assert!(!gen_none.iter().any(|m| same_root_move(m, &mop)));
        // Loud SimpleTake clears the floor and is kept even without prev_to.
        assert!(gen_none.iter().any(|m| same_root_move(m, &loud_land)));
        let gen_dest = generate_quiescence_captures(&state, &weights, Some(loud_land.to));
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
            for &(from_rank, to_rank, color) in &[
                (10u8, 11u8, Color::Black),
                (25u8, 24u8, Color::White),
            ] {
                state.set_current_turn(color);
                let from = Position::new(file, from_rank).unwrap();
                let to = Position::new(file, to_rank).unwrap();
                if state.get_board().get_piece(from).map(|p| p.piece_type)
                    == Some(PieceType::Pawn)
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
            caps.iter().any(|m| m.is_two_step() && move_captures_enemy(&state, m)),
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
            quiet_ml
                .iter()
                .all(|m| !move_captures_enemy(&state, m)),
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
            },
        );
        assert!(result.best_move.is_some());
        assert!(!result.root_lines.is_empty());
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
        let gen = generate_quiescence_captures(&state, &weights, Some(landing));
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
    fn landing_victim_outranks_path_sum_in_sort_key() {
        // Sort key primary is landing victim: a 4000 landing beats a 3000 path sum
        // with landing 50 when comparing the same way quiesce sorts.
        let landing_hi = 4000.0f32;
        let landing_lo = 50.0f32;
        let path_hi = 3000.0f32;
        let path_lo = 500.0f32;
        let key = |landing: f32, path: f32, recapture: bool| {
            (
                (landing * 1000.0).round() as i32,
                recapture,
                (path * 1000.0).round() as i32,
            )
        };
        assert!(key(landing_hi, path_lo, false) > key(landing_lo, path_hi, false));
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
        let quiet = Move::new(
            Position::new(5, 10).unwrap(),
            Position::new(5, 11).unwrap(),
        );
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
        let mut hang_cache = HashMap::new();
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
        assert!((floor - 1200.0).abs() < 1e-3);
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
        let gen = generate_quiescence_captures(&state, &weights, None);
        let gen_pathish = gen
            .iter()
            .filter(|m| quiesce_move_looks_path_or_multileg(&state, m))
            .count();
        eprintln!(
            "opening captures_only pathish={pathish} q_gen={} q_gen_pathish={gen_pathish}",
            gen.len()
        );
        // Without a contested square, PathClear/MultiLeg must not enter q gen.
        assert_eq!(gen_pathish, 0);
        assert!(pathish > 0, "opening should have path-clear captures to filter");
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
        let worth = generate_quiescence_captures(&state, &weights, None).len();
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
        let with_q = probe_quiescence(
            &state,
            &weights,
            4,
            QPruneMode::PathAware,
            None,
        );
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

        let mut cache = HashMap::new();
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
            },
        );
        assert_ne!(
            result.best_move.as_ref().map(|m| (m.from, m.to)),
            Some((guarded.from, guarded.to)),
            "depth-1 must not pick the hanging GG capture"
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
        let mut cache = HashMap::new();
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
}


