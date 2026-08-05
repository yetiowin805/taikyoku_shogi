//! Directional irreversibility for the 500-move draw counter.
//!
//! A quiet step along direction D (or jump Δ) is irreversible when the piece
//! cannot move in the opposite direction / offset under its movement config.

use crate::attack_utils::get_direction_toward;
use crate::movement::direction::{
    direction_set_contains, Direction, DirectionSet, DIRECTION_SET_ALL,
};
use crate::movement::generator::MovementGenerator;
use crate::movement::types::MovementCapability;
use crate::movement::MovementConfig;
use crate::piece::{Color, Piece};
use crate::position::Position;

/// Collect color-adjusted direction bits and jump offsets the piece can use.
fn collect_reach(
    capabilities: &[MovementCapability],
    color: Color,
    dirs: &mut DirectionSet,
    jumps: &mut Vec<(i8, i8)>,
) {
    for cap in capabilities {
        match cap {
            MovementCapability::Simple { directions, .. }
            | MovementCapability::Range { directions, .. }
            | MovementCapability::ConditionalDiagonalJump { directions, .. } => {
                *dirs |= MovementGenerator::adjust_directions_for_color(*directions, color);
            }
            MovementCapability::Jumping { offsets } => {
                jumps.extend(offsets.iter().copied());
            }
            MovementCapability::TwoStep { first, second } => {
                collect_reach(std::slice::from_ref(first.as_ref()), color, dirs, jumps);
                collect_reach(std::slice::from_ref(second.as_ref()), color, dirs, jumps);
            }
            MovementCapability::FreeEagleMultiMove { .. } => {
                // FE can step in every compass direction (limits differ by ray).
                *dirs |= DIRECTION_SET_ALL;
            }
        }
    }
}

fn piece_reach(piece: &Piece) -> (DirectionSet, Vec<(i8, i8)>) {
    let config = MovementConfig::for_piece(piece);
    let mut dirs: DirectionSet = 0;
    let mut jumps = Vec::new();
    collect_reach(&config.capabilities, piece.color, &mut dirs, &mut jumps);
    (dirs, jumps)
}

/// True when moving `from → to` cannot be undone by a move of this piece in the
/// opposite direction/offset (ignoring board occupancy).
pub fn move_is_directionally_irreversible(piece: &Piece, from: Position, to: Position) -> bool {
    if from == to {
        return false;
    }
    let (dirs, jumps) = piece_reach(piece);

    if let Some(dir) = get_direction_toward(from, to) {
        return !direction_set_contains(dirs, dir.opposite());
    }

    let df = to.file as i8 - from.file as i8;
    let dr = to.rank as i8 - from.rank as i8;
    let opposite = (-df, -dr);
    !jumps.iter().any(|&o| o == opposite)
}

/// True if any leg of a multi-step path is directionally irreversible.
pub fn path_has_irreversible_leg(piece: &Piece, path: &[Position]) -> bool {
    for i in 1..path.len() {
        if move_is_directionally_irreversible(piece, path[i - 1], path[i]) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::{GameState, Move};
    use crate::piece::PieceType;

    #[test]
    fn pawn_forward_is_irreversible() {
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(10, 11).unwrap();
        let pawn = Piece::new(PieceType::Pawn, Color::Black, from);
        assert!(move_is_directionally_irreversible(&pawn, from, to));
    }

    #[test]
    fn white_pawn_forward_is_irreversible() {
        let from = Position::new(10, 25).unwrap();
        let to = Position::new(10, 24).unwrap(); // White forward = decreasing rank
        let pawn = Piece::new(PieceType::Pawn, Color::White, from);
        assert!(move_is_directionally_irreversible(&pawn, from, to));
    }

    #[test]
    fn king_orthogonal_is_reversible() {
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(10, 11).unwrap();
        let king = Piece::new(PieceType::King, Color::Black, from);
        assert!(!move_is_directionally_irreversible(&king, from, to));
    }

    #[test]
    fn pawn_quiet_forward_resets_draw_counter() {
        let mut state = GameState::new();
        state.clear_board();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(10, 11).unwrap();
        state.place_piece(Piece::new(PieceType::Pawn, Color::Black, from));
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
        state.set_current_turn(Color::Black);
        state.set_turns_without_capture_or_promotion(40);
        state.reset_rep_history();

        let turn = state.get_current_turn();
        let _ = state.make_move(Move::new(from, to));
        assert_ne!(state.get_current_turn(), turn);
        assert_eq!(state.get_turns_without_capture_or_promotion(), 0);
    }

    #[test]
    fn king_quiet_step_increments_draw_counter() {
        let mut state = GameState::new();
        state.clear_board();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(10, 11).unwrap();
        state.place_piece(Piece::new(PieceType::King, Color::Black, from));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        state.set_turns_without_capture_or_promotion(7);
        state.reset_rep_history();

        let _ = state.make_move(Move::new(from, to));
        assert_eq!(state.get_turns_without_capture_or_promotion(), 8);
    }

    #[test]
    fn capture_still_resets_draw_counter() {
        let mut state = GameState::new();
        state.clear_board();
        let from = Position::new(10, 10).unwrap();
        let to = Position::new(10, 11).unwrap();
        state.place_piece(Piece::new(PieceType::King, Color::Black, from));
        state.place_piece(Piece::new(PieceType::Pawn, Color::White, to));
        state.place_piece(Piece::new(
            PieceType::King,
            Color::White,
            Position::new(20, 20).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        state.set_turns_without_capture_or_promotion(99);
        state.reset_rep_history();

        let _ = state.make_move(Move::new(from, to));
        assert_eq!(state.get_turns_without_capture_or_promotion(), 0);
    }

    #[test]
    fn peacock_forward_diagonal_reversible_via_simple_back() {
        // TwoStep forward + Simple back → opposite exists in the union.
        let from = Position::new(10, 10).unwrap();
        let mid = Position::new(12, 12).unwrap();
        let peacock = Piece::new(PieceType::Peacock, Color::Black, from);
        assert!(!move_is_directionally_irreversible(&peacock, from, mid));
    }
}
