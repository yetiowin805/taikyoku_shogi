//! Shallow alpha-beta search over GameState clones, with compact search traces for the GUI.

use crate::eval::{evaluate_with_ply, EvalWeights};
use crate::game_state::{GameState, Move};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Max root moves kept in the GUI tree (best + alternatives).
pub const MAX_TREE_ROOT_CHILDREN: usize = 12;
/// Max children kept under a non-root tree node.
pub const MAX_TREE_BRANCH: usize = 8;

#[derive(Debug, Clone)]
pub struct SearchConfig {
    pub depth: u32,
    pub max_time_ms: Option<u64>,
    /// When true, build multipv root lines + reply trees for the GUI.
    /// Does not change which move is selected as best.
    pub collect_trace: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            depth: 2,
            max_time_ms: None,
            collect_trace: false,
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
}

struct SearchContext {
    deadline: Option<Instant>,
    nodes: u64,
    abort: bool,
    /// Ply counter for eval noise (does not rely on move_history during search).
    ply: usize,
}

/// Pick a move with alpha-beta (no GUI trace by default).
pub fn search(state: &GameState, weights: &EvalWeights, config: &SearchConfig) -> SearchResult {
    let root_ply = state.get_move_history().len();
    let static_eval = evaluate_with_ply(state, weights, root_ply);
    let deadline = config
        .max_time_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    let mut ctx = SearchContext {
        deadline,
        nodes: 0,
        abort: false,
        ply: root_ply,
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
        };
    }

    order_moves(state, weights, &mut moves);

    let depth = config.depth.max(1);
    let mut best_move = moves[0].clone();
    let mut best_score = i32::MIN + 1;
    let mut alpha = i32::MIN + 1;
    let beta = i32::MAX - 1;
    let mut root_lines: Vec<(Move, i32)> = Vec::with_capacity(moves.len());

    for mv in &moves {
        if ctx.timed_out() {
            break;
        }
        let mut child = state.clone();
        if !child.make_move_for_search(mv.clone()) {
            continue;
        }
        ctx.nodes += 1;
        ctx.ply = root_ply + 1;

        let score = -alphabeta(&mut child, weights, depth - 1, -beta, -alpha, &mut ctx);
        root_lines.push((mv.clone(), score));

        if score > best_score {
            best_score = score;
            best_move = mv.clone();
        }
        if score > alpha {
            alpha = score;
        }
    }

    root_lines.sort_by(|a, b| b.1.cmp(&a.1));

    let tree = if config.collect_trace {
        build_trace_tree(
            state,
            weights,
            depth,
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
    }
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
            if child.make_move_for_search(best_move.clone()) {
                ctx.ply = root_ply + 1;
                let (_score, subtree) =
                    alphabeta_record(&mut child, weights, depth - 1, &mut *ctx);
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

    // Terminal / leaf: evaluate only here (correctness-preserving for plain AB).
    if state.get_winner().is_some() || depth == 0 || ctx.timed_out() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }

    let mut moves = state.generate_legal_moves();
    if moves.is_empty() {
        return evaluate_with_ply(state, weights, ctx.ply);
    }

    order_moves(state, weights, &mut moves);

    let mut best = i32::MIN + 1;
    let parent_ply = ctx.ply;

    for mv in moves {
        if ctx.timed_out() {
            break;
        }
        let mut child_state = state.clone();
        if !child_state.make_move_for_search(mv) {
            continue;
        }
        ctx.ply = parent_ply + 1;
        let score = -alphabeta(&mut child_state, weights, depth - 1, -beta, -alpha, ctx);
        ctx.ply = parent_ply;

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

/// Like alphabeta but records reply nodes for the GUI (best-move expansion only).
fn alphabeta_record(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    ctx: &mut SearchContext,
) -> (i32, Option<SearchTreeNode>) {
    ctx.nodes += 1;
    let static_eval = evaluate_with_ply(state, weights, ctx.ply);

    if state.get_winner().is_some() || depth == 0 || ctx.timed_out() {
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
        let mut child_state = state.clone();
        if !child_state.make_move_for_search(mv) {
            continue;
        }
        ctx.ply = parent_ply + 1;
        let score = -alphabeta(&mut child_state, weights, depth - 1, -beta, -alpha, ctx);
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
    fn timed_out(&mut self) -> bool {
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
    let board = state.get_board();
    moves.sort_by_key(|mv| {
        let capture_val = board
            .get_piece(mv.to)
            .map(|p| weights.piece_value(p.piece_type))
            .unwrap_or(0);
        -capture_val
    });
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
            },
        );
        let best = result.best_move.expect("expected a move");
        assert_eq!(best.to, Position::new(10, 11).unwrap());
        assert!(result.score > 100_000, "mate-ish score, got {}", result.score);
        assert!(!result.tree.children.is_empty());
    }

    #[test]
    fn play_and_trace_agree_on_best_move() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let play = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                collect_trace: false,
                ..SearchConfig::default()
            },
        );
        let traced = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 1,
                collect_trace: true,
                ..SearchConfig::default()
            },
        );
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
            let ok_search = a.make_move_for_search(mv.clone());
            let _ = b.make_move(mv.clone());
            assert!(ok_search, "search apply failed for {:?}", mv);
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
        }
    }

    #[test]
    fn opening_depth2_play_search_completes() {
        let weights = EvalWeights::seed();
        let mut state = GameState::new();
        state.setup_initial_position();
        let t0 = Instant::now();
        let result = search(
            &state,
            &weights,
            &SearchConfig {
                depth: 2,
                max_time_ms: None,
                collect_trace: false,
            },
        );
        let elapsed = t0.elapsed();
        assert!(result.best_move.is_some());
        assert!(result.nodes > 0);
        // Soft sanity: depth-2 opening should finish well under a few seconds after speedups.
        assert!(
            elapsed.as_secs() < 5,
            "opening depth-2 took {:?}, nodes={}",
            elapsed,
            result.nodes
        );
        eprintln!(
            "opening depth-2 play: {:?} nodes={} score={}",
            elapsed, result.nodes, result.score
        );
    }
}
