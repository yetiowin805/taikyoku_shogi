//! Oracle differential harness: today's capability stack is the source of truth.
//!
//! Faster paths (victim-square capture gen, directed reach) must match these checks.

use crate::game_state::{GameState, LegalMoveGen, Move, MoveData};
use crate::movement::{MovementConfig, MovementGenerator};
use crate::piece::Color;
use crate::position::Position;
use crate::search::{capture_hits_square, generate_captures_hitting_square};
use std::collections::BTreeSet;

/// Canonical move identity for parity (includes two-step / FE structure).
fn move_key(mv: &Move) -> String {
    let kind = match &mv.data {
        MoveData::Standard => "S",
        MoveData::TwoStep { .. } => "T",
        MoveData::FreeEagle { .. } => "F",
    };
    let mut s = format!(
        "{}:{}:{}:{}",
        mv.from.to_index(),
        mv.to.to_index(),
        mv.promoted as u8,
        kind
    );
    if let Some(inter) = mv.intermediate() {
        s.push_str(&format!(":i{}", inter.to_index()));
    }
    if let Some(path) = mv.free_eagle_path() {
        s.push_str(":p");
        for p in path {
            s.push_str(&format!(",{}", p.to_index()));
        }
    }
    s
}

fn move_set(moves: &[Move]) -> BTreeSet<String> {
    moves.iter().map(move_key).collect()
}

/// Full-board CapturesOnly filtered to hits on `victim` (oracle for victim-square gen).
pub fn oracle_captures_hitting_square(state: &GameState, victim: Position) -> Vec<Move> {
    state
        .generate_legal_moves_mode(LegalMoveGen::CapturesOnly)
        .into_iter()
        .filter(|mv| capture_hits_square(state, mv, victim))
        .collect()
}

fn assert_same_moves(label: &str, a: &[Move], b: &[Move]) {
    let sa = move_set(a);
    let sb = move_set(b);
    if sa != sb {
        let only_a: Vec<_> = sa.difference(&sb).take(5).cloned().collect();
        let only_b: Vec<_> = sb.difference(&sa).take(5).cloned().collect();
        panic!(
            "{label}: move sets differ; only_oracle~{:?} only_fast~{:?} (n {} vs {})",
            only_a,
            only_b,
            a.len(),
            b.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opening() -> GameState {
        let mut s = GameState::new();
        s.setup_initial_position();
        s
    }

    #[test]
    fn victim_square_gen_matches_oracle_opening_samples() {
        let state = opening();
        let board = state.get_board();
        let victims: Vec<Position> = board
            .iter_pieces_by_color(Color::White)
            .take(12)
            .map(|p| p.position)
            .collect();
        for victim in victims {
            let oracle = oracle_captures_hitting_square(&state, victim);
            let fast = generate_captures_hitting_square(&state, victim);
            assert_same_moves(
                &format!("opening victim {}", victim.to_index()),
                &oracle,
                &fast,
            );
        }
    }

    #[test]
    fn directed_reach_matches_full_gen_on_opening() {
        let state = opening();
        let board = state.get_board();
        let mut checked = 0u32;
        for piece in board.iter_pieces_by_color(Color::Black).take(40) {
            let config = MovementConfig::for_piece(&piece);
            let mut probes = Vec::new();
            for d in 1..=3i8 {
                for (df, dr) in [
                    (0, d),
                    (0, -d),
                    (d, 0),
                    (-d, 0),
                    (d, d),
                    (d, -d),
                    (-d, d),
                    (-d, -d),
                ] {
                    if let Some(t) = piece.position.offset(df, dr) {
                        probes.push(t);
                    }
                }
            }
            for target in probes {
                let full = MovementGenerator::generate_targets(&piece, board, &config.capabilities)
                    .contains(&target);
                let directed =
                    MovementGenerator::can_reach_target(&piece, board, &config.capabilities, target);
                assert_eq!(
                    full, directed,
                    "reach mismatch {:?} -> {:?} full={} directed={}",
                    piece.position, target, full, directed
                );
                checked += 1;
            }
        }
        assert!(checked > 100, "should probe many squares, got {checked}");
    }

    #[test]
    fn make_unmake_fuzz_opening_captures() {
        let mut state = opening();
        let moves = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
        let sample: Vec<_> = moves.into_iter().take(32).collect();
        for mv in sample {
            let before = state.hash();
            let turn_before = state.get_current_turn();
            let n_before = state.get_board().iter_pieces_by_color(Color::Black).count()
                + state.get_board().iter_pieces_by_color(Color::White).count();
            let Some(undo) = state.make_move_for_search(mv.clone()) else {
                continue;
            };
            state.unmake_move_for_search(undo);
            assert_eq!(state.hash(), before, "hash after unmake");
            assert_eq!(state.get_current_turn(), turn_before);
            let n_after = state.get_board().iter_pieces_by_color(Color::Black).count()
                + state.get_board().iter_pieces_by_color(Color::White).count();
            assert_eq!(n_after, n_before);
        }
    }

    #[test]
    fn attack_implies_can_reach_for_simple_captures() {
        let state = opening();
        let caps = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
        let mut n = 0;
        for mv in caps.iter().take(50) {
            if mv.is_two_step() || mv.is_free_eagle() {
                continue;
            }
            let Some(piece) = state.get_board().get_piece(mv.from) else {
                continue;
            };
            assert!(
                piece.can_reach(mv.to, state.get_board()),
                "mover should reach capture dest {:?}",
                mv.to
            );
            n += 1;
        }
        assert!(n > 0);
    }

    #[test]
    fn victim_square_filters_piece_subset() {
        let state = opening();
        let victim = Position::new(18, 20).unwrap();
        let us = state.get_current_turn();
        let all = state.get_board().iter_pieces_by_color(us).count();
        let filtered = state
            .get_board()
            .iter_pieces_by_color(us)
            .filter(|p| {
                crate::attack_utils::should_check_piece_for_target_position(p, victim, false)
            })
            .count();
        assert!(filtered <= all);
        assert!(filtered < all, "proximity should drop some opening pieces");
    }
}
