//! Shallow alpha-beta search over GameState clones, with compact search traces for the GUI.

use crate::eval::{evaluate, EvalWeights};
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
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            depth: 2,
            max_time_ms: None,
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
}

struct RecordedChild {
    mv: Move,
    label: String,
    score: i32,
    subtree: Option<SearchTreeNode>,
}

/// Pick a move with alpha-beta.
pub fn search(state: &GameState, weights: &EvalWeights, config: &SearchConfig) -> SearchResult {
    let static_eval = evaluate(state, weights);
    let deadline = config
        .max_time_ms
        .map(|ms| Instant::now() + Duration::from_millis(ms));

    let mut ctx = SearchContext {
        deadline,
        nodes: 0,
        abort: false,
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
    let mut recorded: Vec<RecordedChild> = Vec::with_capacity(moves.len());

    for mv in &moves {
        if ctx.timed_out() {
            break;
        }
        let label = move_label(state, mv);
        let mut child = state.clone();
        child.make_move(mv.clone());
        ctx.nodes += 1;

        let (raw, subtree) = alphabeta(
            &mut child,
            weights,
            depth - 1,
            -beta,
            -alpha,
            &mut ctx,
            depth > 1,
        );
        let score = -raw;
        root_lines.push((mv.clone(), score));
        recorded.push(RecordedChild {
            mv: mv.clone(),
            label,
            score,
            subtree,
        });

        if score > best_score {
            best_score = score;
            best_move = mv.clone();
        }
        if score > alpha {
            alpha = score;
        }
    }

    recorded.sort_by(|a, b| b.score.cmp(&a.score));

    let tree_children: Vec<SearchTreeNode> = recorded
        .iter()
        .take(MAX_TREE_ROOT_CHILDREN)
        .map(|r| {
            let is_best = r.mv.from == best_move.from
                && r.mv.to == best_move.to
                && r.mv.promoted == best_move.promoted;
            let mut children = r
                .subtree
                .as_ref()
                .map(|t| t.children.clone())
                .unwrap_or_default();
            if children.len() > MAX_TREE_BRANCH {
                children.sort_by(|a, b| {
                    b.best
                        .cmp(&a.best)
                        .then(b.score.unwrap_or(i32::MIN).cmp(&a.score.unwrap_or(i32::MIN)))
                });
                children.truncate(MAX_TREE_BRANCH);
            }
            SearchTreeNode {
                label: r.label.clone(),
                score: Some(r.score),
                static_eval: None,
                best: is_best,
                cutoff: false,
                children,
            }
        })
        .collect();

    let tree = SearchTreeNode {
        label: "root".into(),
        score: Some(best_score),
        static_eval: Some(static_eval),
        best: true,
        cutoff: false,
        children: tree_children,
    };

    root_lines.sort_by(|a, b| b.1.cmp(&a.1));

    SearchResult {
        best_move: Some(best_move),
        score: best_score,
        nodes: ctx.nodes,
        static_eval,
        root_lines,
        tree,
    }
}

fn alphabeta(
    state: &mut GameState,
    weights: &EvalWeights,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    ctx: &mut SearchContext,
    record: bool,
) -> (i32, Option<SearchTreeNode>) {
    ctx.nodes += 1;
    let static_eval = evaluate(state, weights);

    if state.get_winner().is_some() || depth == 0 || ctx.timed_out() {
        return (
            static_eval,
            if record {
                Some(SearchTreeNode {
                    label: "eval".into(),
                    score: Some(static_eval),
                    static_eval: Some(static_eval),
                    best: true,
                    cutoff: false,
                    children: vec![],
                })
            } else {
                None
            },
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

    for mv in moves {
        if ctx.timed_out() {
            break;
        }
        let label = move_label(state, &mv);
        let mut child_state = state.clone();
        child_state.make_move(mv);
        let (raw, _) = alphabeta(
            &mut child_state,
            weights,
            depth - 1,
            -beta,
            -alpha,
            ctx,
            false,
        );
        let score = -raw;

        if score > best {
            best = score;
            best_label = Some(label.clone());
        }
        if score > alpha {
            alpha = score;
        }

        let cutoff = alpha >= beta;
        if record {
            children.push(SearchTreeNode {
                label,
                score: Some(score),
                static_eval: None,
                best: false,
                cutoff,
                children: vec![],
            });
        }
        if cutoff {
            break;
        }
    }

    if record {
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
        return (
            best,
            Some(SearchTreeNode {
                label: "replies".into(),
                score: Some(best),
                static_eval: Some(static_eval),
                best: true,
                cutoff: false,
                children,
            }),
        );
    }

    (best, None)
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
            },
        );
        let best = result.best_move.expect("expected a move");
        assert_eq!(best.to, Position::new(10, 11).unwrap());
        assert!(result.score > 100_000, "mate-ish score, got {}", result.score);
        assert!(!result.tree.children.is_empty());
    }
}
