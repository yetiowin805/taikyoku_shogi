//! Alpha-beta search over GameState with make/unmake, compact traces for the GUI.

use crate::eval::{evaluate_with_ply, EvalWeights};
use crate::game_state::{GameState, LegalMoveGen, Move};
use crate::movement::{BlockingMode, MovementCapability, MovementConfig};
use crate::path_utils;
use crate::piece::Color;
use crate::position::Position;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Max root moves kept in the GUI tree (best + alternatives).
pub const MAX_TREE_ROOT_CHILDREN: usize = 12;
/// Max children kept under a non-root tree node.
pub const MAX_TREE_BRANCH: usize = 8;

/// Experimental / production quiescence capture filters.
///
/// Measurement (`qprune_mode_matrix`, post-GG leaf q=6):
/// - **TopN**: ~4.4× fewer q-nodes, score ~5 vs baseline 7
/// - **NetGain**: little leaf effect; fewer AB nodes on opening
/// - **RecaptureOnly**: ~200× cut but score 90 vs 7 (over-prune) — not default
/// - **StaleHang**: no meaningful cut on the GG blowup
///
/// Default: [`QPruneMode::NetGainAndTopN`].
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
    /// D: drop captures where landing looks attacked and enemy < mover (stale).
    StaleHang,
    /// A+B — shipped default after measurement.
    #[default]
    NetGainAndTopN,
    /// B+C (kept for harness; recapture over-pruned alone).
    TopNAndRecapture,
}

/// Max captures expanded per q-node in TopN modes.
pub const QUIESCE_TOP_N: usize = 8;

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
            // Measurement (qprune_mode_matrix): TopN ~4× on GG leaves with
            // near-stable score; net-gain is free and helps AB node counts.
            // Recapture-only over-pruned (score 90 vs baseline 7) — not default.
            q_prune_mode: QPruneMode::NetGainAndTopN,
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
    /// Two killer quiets per ply (cutoff memory for capture-setups).
    killers: Vec<[Option<MoveKey>; 2]>,
    /// Quiet history: (from_index << 16) | to_index → score.
    history: HashMap<u32, i32>,
    /// Disallow consecutive null moves.
    allow_null: bool,
    /// Quiescence diagnostics (updated while in `quiesce`).
    q_nodes: u64,
    q_depth_left: u32,
    q_caps_at_node: usize,
    q_cap_index: usize,
    q_label: String,
    q_stand_pat: i32,
    q_prune_mode: QPruneMode,
    q_caps_generated: u64,
    q_caps_searched: u64,
}

impl QPruneMode {
    fn uses_net_gain(self) -> bool {
        matches!(self, Self::NetGainDelta | Self::NetGainAndTopN)
    }

    fn uses_top_n(self) -> bool {
        matches!(
            self,
            Self::TopN | Self::NetGainAndTopN | Self::TopNAndRecapture
        )
    }

    fn uses_recapture_only(self) -> bool {
        matches!(self, Self::RecaptureOnly | Self::TopNAndRecapture)
    }

    fn uses_stale_hang(self) -> bool {
        matches!(self, Self::StaleHang)
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
    // Fast non-cryptographic board hash (not incremental; good enough for TT keys).
    let mut h = match state.get_current_turn() {
        Color::Black => 0xA5A5_A5A5_A5A5_A5A5u64,
        Color::White => 0x5A5A_5A5A_5A5A_5A5Au64,
    };
    h ^= (state.get_turns_without_capture_or_promotion() as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let board = state.get_board();
    for color in [Color::Black, Color::White] {
        for p in board.pieces_by_color(color) {
            let mut x = (p.piece_type as u64).wrapping_mul(0x1000_0000_1B3);
            x ^= (p.position.file as u64) << 24;
            x ^= (p.position.rank as u64) << 16;
            x ^= (p.is_promoted as u64) << 8;
            x ^= match color {
                Color::Black => 1,
                Color::White => 2,
            };
            h ^= x.wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
            h = h.rotate_left(13);
        }
    }
    h
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
) -> (i32, i32) {
    if mv.from == mv.to {
        return (0, 0);
    }
    let board = state.get_board();
    let Some(piece) = board.get_piece(mv.from) else {
        return (0, 0);
    };
    let us = piece.color;
    let them = us.opposite();
    let mut enemy = 0i32;
    let mut own = 0i32;

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

/// Minimum enemy material (eval points) for a capture to enter quiescence.
/// With seed weights, one jump-capture general / high piece is 90–100.
pub const MIN_QUIESCENCE_ENEMY_MATERIAL: i32 = 50;

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
    enemy >= MIN_QUIESCENCE_ENEMY_MATERIAL
}

/// MVV-LVA capture score without hang checks (for quiescence ordering).
fn mvv_lva_score(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    let mover_value = weights.piece_value(mover.piece_type);
    let (enemy, own) = capture_material_exchange(state, weights, mv);
    if enemy == 0 {
        return i32::MIN / 4;
    }
    (enemy - own)
        .saturating_mul(1000)
        .saturating_sub(mover_value)
}

/// Move-ordering score (heuristic only — not search correctness).
///
/// Captures: `gain = enemy - own`, then if the landing square looks attacked by
/// the opponent on the **current** (pre-move) board, subtract mover value
/// (stale hang estimate; ignores path clears). LVA tie-break: `gain*1000 - mover`.
/// Quiets sort below captures. `attack_cache` is per-`order_moves` call.
fn move_order_score(
    state: &GameState,
    weights: &EvalWeights,
    mv: &Move,
    opponent: Color,
    attack_cache: &mut HashMap<usize, bool>,
) -> i32 {
    let board = state.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return i32::MIN / 4;
    };
    let mover_value = weights.piece_value(mover.piece_type);
    let (enemy, own) = capture_material_exchange(state, weights, mv);
    if enemy == 0 {
        return i32::MIN / 4;
    }

    let mut gain = enemy - own;
    if landing_attacked_cached(board, mv.to, opponent, attack_cache) {
        gain -= mover_value;
    }
    gain.saturating_mul(1000).saturating_sub(mover_value)
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

/// Captures worth expanding in quiescence.
///
/// Uses capture-oriented generation (no quiet ray fan-out / quiet multi-leg).
fn generate_quiescence_captures(state: &GameState, weights: &EvalWeights) -> Vec<Move> {
    state
        .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
        .into_iter()
        .filter(|mv| is_worthwhile_quiescence_capture(state, weights, mv))
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
        search_started: now,
        last_progress_log: now,
        search_depth: max_depth,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "root",
        tt: TranspositionTable::new(1 << 20),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        q_nodes: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: config.q_prune_mode,
        q_caps_generated: 0,
        q_caps_searched: 0,
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

    order_moves_with_heuristics(state, weights, &mut moves, &ctx, root_ply, false);
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
            ctx.maybe_log_progress();

            let is_capture = move_captures_enemy(state, mv);
            let child_depth = d - 1;
            // Root LMR: late quiets at ID depth >= 2.
            let can_reduce = d >= 2 && i >= 3 && !is_capture && child_depth >= 1;
            let reduction = if can_reduce {
                (if i >= 12 { 2 } else { 1 }).min(child_depth)
            } else {
                0
            };

            let Some(undo) = pos.make_move_for_search(mv.clone()) else {
                continue;
            };
            ctx.nodes += 1;
            ctx.ply = root_ply + 1;
            ctx.phase = "search";
            ctx.q_nodes = 0;
            ctx.q_label.clear();
            ctx.q_caps_at_node = 0;
            ctx.q_cap_index = 0;

            let mut score = if reduction > 0 {
                let reduced = child_depth - reduction;
                -alphabeta(&mut pos, weights, reduced, -beta, -alpha, &mut ctx)
            } else {
                -alphabeta(&mut pos, weights, child_depth, -beta, -alpha, &mut ctx)
            };
            if reduction > 0 && !ctx.abort && score > alpha {
                score = -alphabeta(&mut pos, weights, child_depth, -beta, -alpha, &mut ctx);
            }

            pos.unmake_move_for_search(undo);
            ctx.ply = root_ply;

            if ctx.abort {
                finished_iteration = false;
                break;
            }
            iter_lines.push((mv.clone(), score));

            if score > iter_score {
                iter_score = score;
                iter_best = mv.clone();
                ctx.best_score = iter_score;
                if !is_capture {
                    store_killer(&mut ctx, root_ply, move_tt_key(mv));
                    bump_history(&mut ctx, mv, d);
                }
            }
            if score > alpha {
                alpha = score;
            }
        }

        if !finished_iteration {
            // Keep last completed iteration (partial d=1 only if nothing completed yet).
            if completed_depth == 0 && !iter_lines.is_empty() {
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
        search_started: now,
        last_progress_log: now,
        search_depth: 0,
        root_index: 0,
        root_total: 0,
        root_label: String::new(),
        best_score: i32::MIN + 1,
        phase: "quiesce",
        tt: TranspositionTable::new(1024),
        killers: Vec::new(),
        history: HashMap::new(),
        allow_null: true,
        q_nodes: 0,
        q_depth_left: 0,
        q_caps_at_node: 0,
        q_cap_index: 0,
        q_label: String::new(),
        q_stand_pat: 0,
        q_prune_mode: mode,
        q_caps_generated: 0,
        q_caps_searched: 0,
    };
    let mut pos = state.clone();
    let score = if qdepth == 0 {
        static_eval
    } else {
        quiesce(
            &mut pos,
            weights,
            qdepth,
            i32::MIN + 1,
            i32::MAX - 1,
            None,
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
    ctx: &mut SearchContext,
) -> i32 {
    ctx.nodes += 1;

    if state.get_winner().is_some() || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }
    if depth == 0 {
        return leaf_or_quiesce(state, weights, alpha, beta, ctx);
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
    const NULL_R: u32 = 2;
    const MATE_BAND: i32 = 900_000;
    if ctx.allow_null && depth >= 2 && beta < MATE_BAND && beta > -MATE_BAND {
        let r = NULL_R.min(depth - 1);
        ctx.allow_null = false;
        let prev_turn = state.get_current_turn();
        state.set_current_turn(prev_turn.opposite());
        let parent_ply = ctx.ply;
        ctx.ply = parent_ply + 1;
        let null_depth = depth - 1 - r;
        let score = -alphabeta(state, weights, null_depth, -beta, -beta + 1, ctx);
        ctx.ply = parent_ply;
        state.set_current_turn(prev_turn);
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
        order_moves_with_heuristics(state, weights, &mut moves, ctx, ctx.ply, true);
    } else {
        order_moves_with_heuristics(state, weights, &mut moves, ctx, ctx.ply, false);
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
        ctx,
        parent_ply,
        moves,
        0,
    );

    if !did_cutoff && !ctx.abort && !used_only_stage_b {
        let mut stage_b = state.generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly);
        if !stage_b.is_empty() {
            order_moves_with_heuristics(state, weights, &mut stage_b, ctx, parent_ply, true);
            prefer_tt_move(&mut stage_b, tt_move);
            let (b2, k2, a2, _cut2) = search_move_list(
                state,
                weights,
                depth,
                alpha,
                beta,
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
    ctx: &mut SearchContext,
    parent_ply: usize,
    moves: Vec<Move>,
    move_index_base: usize,
) -> (i32, Option<MoveKey>, i32, bool) {
    let mut best = i32::MIN + 1;
    let mut best_move_key = None;
    let mut did_cutoff = false;

    for (i, mv) in moves.into_iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        let mv_key = move_tt_key(&mv);
        let is_capture = move_captures_enemy(state, &mv);
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

        let Some(undo) = state.make_move_for_search(mv.clone()) else {
            continue;
        };
        ctx.ply = parent_ply + 1;

        let mut score = if reduction > 0 {
            let reduced = depth - 1 - reduction;
            -alphabeta(state, weights, reduced, -beta, -alpha, ctx)
        } else {
            -alphabeta(state, weights, depth - 1, -beta, -alpha, ctx)
        };

        // Re-search at full depth if reduced search looks interesting.
        if reduction > 0 && !ctx.abort && score > alpha {
            score = -alphabeta(state, weights, depth - 1, -beta, -alpha, ctx);
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

fn leaf_or_quiesce(
    state: &mut GameState,
    weights: &EvalWeights,
    alpha: i32,
    beta: i32,
    ctx: &mut SearchContext,
) -> i32 {
    if ctx.quiescence_depth == 0 {
        evaluate_with_ply(state, weights, ctx.ply)
    } else {
        ctx.phase = "quiesce";
        quiesce(
            state,
            weights,
            ctx.quiescence_depth,
            alpha,
            beta,
            None,
            ctx,
        )
    }
}

/// Capture-only quiescence (excludes pure self-captures via `move_captures_enemy`).
///
/// `recapture_sq`: when set (deeper q-plies under RecaptureOnly modes), only
/// expands captures that take on that square.
fn quiesce(
    state: &mut GameState,
    weights: &EvalWeights,
    qdepth: u32,
    mut alpha: i32,
    beta: i32,
    recapture_sq: Option<Position>,
    ctx: &mut SearchContext,
) -> i32 {
    ctx.nodes += 1;
    ctx.q_nodes += 1;
    ctx.q_depth_left = qdepth;

    if state.get_winner().is_some() || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }

    let stand_pat = evaluate_with_ply(state, weights, ctx.ply);
    ctx.q_stand_pat = stand_pat;
    if qdepth == 0 {
        return stand_pat;
    }
    if stand_pat >= beta {
        return stand_pat;
    }
    // Optimistic delta against the incoming alpha (before stand-pat raise).
    let optimistic = weights
        .piece_value_table
        .iter()
        .copied()
        .max()
        .unwrap_or(100)
        .max(MIN_QUIESCENCE_ENEMY_MATERIAL);
    if stand_pat.saturating_add(optimistic) <= alpha {
        return stand_pat;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut moves = generate_quiescence_captures(state, weights);
    if moves.is_empty() {
        return stand_pat;
    }

    // Recapture-only after the first q-ply.
    if ctx.q_prune_mode.uses_recapture_only() {
        if let Some(sq) = recapture_sq {
            moves.retain(|mv| capture_hits_square(state, mv, sq));
            if moves.is_empty() {
                return stand_pat;
            }
        }
    }

    let use_net = ctx.q_prune_mode.uses_net_gain();
    // Delta prune: drop captures that cannot raise alpha.
    moves.retain(|mv| {
        let (enemy, own) = capture_material_exchange(state, weights, mv);
        let gain = if use_net {
            enemy.saturating_sub(own)
        } else {
            enemy
        };
        stand_pat.saturating_add(gain) > alpha
    });
    if moves.is_empty() {
        return stand_pat;
    }

    // Stale hang prune (pre-move landing attack).
    if ctx.q_prune_mode.uses_stale_hang() {
        let opponent = state.get_current_turn().opposite();
        let mut attack_cache: HashMap<usize, bool> = HashMap::new();
        moves.retain(|mv| {
            let board = state.get_board();
            let Some(mover) = board.get_piece(mv.from) else {
                return false;
            };
            let mover_value = weights.piece_value(mover.piece_type);
            let (enemy, _) = capture_material_exchange(state, weights, mv);
            if enemy >= mover_value {
                return true;
            }
            !landing_attacked_cached(board, mv.to, opponent, &mut attack_cache)
        });
        if moves.is_empty() {
            return stand_pat;
        }
    }

    order_moves_quiescence(state, weights, &mut moves);

    ctx.q_caps_generated = ctx
        .q_caps_generated
        .saturating_add(moves.len() as u64);

    if ctx.q_prune_mode.uses_top_n() && moves.len() > QUIESCE_TOP_N {
        moves.truncate(QUIESCE_TOP_N);
    }

    let n_caps = moves.len();
    ctx.q_caps_at_node = n_caps;

    let mut best = stand_pat;
    let parent_ply = ctx.ply;

    for (i, mv) in moves.into_iter().enumerate() {
        if ctx.timed_out() {
            break;
        }
        ctx.q_cap_index = i + 1;
        ctx.q_label = move_label(state, &mv);
        ctx.phase = "quiesce";
        // Periodic progress while a single loud capture line is exploding.
        if ctx.q_nodes & 0xff == 0 {
            ctx.maybe_log_progress();
        }

        let next_recapture = if ctx.q_prune_mode.uses_recapture_only() {
            Some(mv.to)
        } else {
            None
        };

        let Some(undo) = state.make_move_for_search(mv) else {
            continue;
        };
        ctx.q_caps_searched += 1;
        ctx.ply = parent_ply + 1;
        let score = -quiesce(
            state,
            weights,
            qdepth - 1,
            -beta,
            -alpha,
            next_recapture,
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
        let Some(undo) = state.make_move_for_search(mv) else {
            continue;
        };
        ctx.ply = parent_ply + 1;
        let score = -alphabeta(state, weights, depth - 1, -beta, -alpha, ctx);
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
        let qinfo = if self.phase == "quiesce" || self.q_nodes > 0 {
            format!(
                " qnodes={} qleft={} qcaps={}/{} qmv={} spat={}",
                self.q_nodes,
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
        eprintln!(
            "ab search: {:.1}s nodes={} nps={} depth={} q={} phase={} root={} best={}{}",
            elapsed.as_secs_f64(),
            self.nodes,
            nps,
            self.search_depth,
            self.quiescence_depth,
            self.phase,
            root,
            best,
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
        ))
    });
}

/// Main-search ordering: captures (hang/MVV) then killers then history for quiets.
fn order_moves_with_heuristics(
    state: &GameState,
    weights: &EvalWeights,
    moves: &mut [Move],
    ctx: &SearchContext,
    ply: usize,
    captures_only_style: bool,
) {
    let opponent = state.get_current_turn().opposite();
    let mut attack_cache: HashMap<usize, bool> = HashMap::new();
    moves.sort_by_key(|mv| {
        let cap = if captures_only_style {
            mvv_lva_score(state, weights, mv)
        } else {
            move_order_score(state, weights, mv, opponent, &mut attack_cache)
        };
        let kr = killer_rank(ctx, ply, mv);
        let hist = history_score(ctx, mv);
        std::cmp::Reverse((cap, kr, hist))
    });
}

/// Quiescence ordering: MVV-LVA only (no attack scans).
fn order_moves_quiescence(state: &GameState, weights: &EvalWeights, moves: &mut [Move]) {
    moves.sort_by_key(|mv| std::cmp::Reverse(mvv_lva_score(state, weights, mv)));
}

/// Test/helper: ordering score with a fresh per-call attack cache.
fn move_order_score_fresh(state: &GameState, weights: &EvalWeights, mv: &Move) -> i32 {
    let opponent = state.get_current_turn().opposite();
    let mut cache = HashMap::new();
    move_order_score(state, weights, mv, opponent, &mut cache)
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
                q_prune_mode: QPruneMode::NetGainAndTopN,
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
            q_prune_mode: QPruneMode::NetGainAndTopN,
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
                q_prune_mode: QPruneMode::NetGainAndTopN,
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
            result.score > -500,
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
                q_prune_mode: QPruneMode::NetGainAndTopN,
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
                q_prune_mode: QPruneMode::NetGainAndTopN,
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
        // Small budget: depth-1 should complete at least one root move in debug.
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 2,
                max_time_ms: Some(250),
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::NetGainAndTopN,
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
        // Seed pawn value is 1 — below the 50-point qsearch threshold.
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
            "taking a 1-point pawn should not enter qsearch"
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
    fn opening_worthwhile_quiescence_captures_far_fewer_than_raw() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let all = state.generate_legal_moves();
        let raw_caps = all
            .iter()
            .filter(|m| move_captures_enemy(&state, m))
            .count();
        let worth = generate_quiescence_captures(&state, &weights).len();
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
        // Fixed mock values: gold >> pawn so hanging the gold is clearly wrong.
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::GoldGeneral, 100);
        weights.piece.insert(PieceType::Pawn, 1);
        weights.piece.insert(PieceType::King, 100);
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
        // Black gold can take white pawn, but white gold then recaptures.
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
            PieceType::GoldGeneral,
            Color::White,
            Position::new(10, 12).unwrap(),
        ));
        state.set_current_turn(Color::Black);

        let hanging = Position::new(10, 11).unwrap();

        let greedy = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 0,
                q_prune_mode: QPruneMode::NetGainAndTopN,
            },
        );
        assert_eq!(
            greedy.best_move.as_ref().map(|m| m.to),
            Some(hanging),
            "without qsearch, depth-1 should greedily take the pawn"
        );

        let with_q = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                max_time_ms: None,
                collect_trace: false,
                quiescence_depth: 4,
                q_prune_mode: QPruneMode::NetGainAndTopN,
            },
        );
        assert_ne!(
            with_q.best_move.as_ref().map(|m| m.to),
            Some(hanging),
            "with qsearch, should not hang the gold for a pawn"
        );
    }

    #[test]
    fn see_orders_safe_landing_above_guarded() {
        let mut weights = EvalWeights::seed();
        weights.noise_scale = 0.0;
        weights.piece.insert(PieceType::GreatGeneral, 90);
        weights.piece.insert(PieceType::Pawn, 1);
        weights.piece.insert(PieceType::GoldGeneral, 50);
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
        weights.piece.insert(PieceType::Pawn, 1);
        weights.piece.insert(PieceType::GoldGeneral, 50);
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


