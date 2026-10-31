//! Permanent directional progress for the progress-draw counter.
//!
//! Missing an immediate reverse move does not imply progress: a Free Tiger,
//! gold, or silver can return by another route. Only count an increase in a
//! linear coordinate that none of the piece's movement capabilities can decrease.

use std::sync::OnceLock;

use crate::movement::direction::direction_iter;
use crate::movement::generator::MovementGenerator;
use crate::movement::types::MovementCapability;
use crate::movement::MovementConfig;
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;

// Rank, file, and the two diagonals suffice for every current movement config.
// Extra movement shapes may conservatively miss progress, but cannot invent it.
const COORDINATES: [(i16, i16); 8] = [
    (0, 1),
    (1, 1),
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
];

fn nondecreasing_coordinates(cap: &MovementCapability, color: Color) -> u8 {
    let for_offset = |x: i16, y: i16| {
        COORDINATES
            .iter()
            .enumerate()
            .fold(0, |mask, (i, &(a, b))| {
                mask | (u8::from(a * x + b * y >= 0) << i)
            })
    };
    match cap {
        MovementCapability::Simple { directions, .. }
        | MovementCapability::Range { directions, .. }
        | MovementCapability::ConditionalDiagonalJump { directions, .. } => direction_iter(
            MovementGenerator::adjust_directions_for_color(*directions, color),
        )
        .fold(u8::MAX, |mask, direction| {
            let (x, y) = direction.to_offset();
            mask & for_offset(i16::from(x), i16::from(y))
        }),
        MovementCapability::Jumping { offsets } => offsets.iter().fold(u8::MAX, |mask, &(x, y)| {
            // Match the generator's full rotation of piece-relative offsets.
            let (x, y) = if color == Color::White {
                (-i16::from(x), -i16::from(y))
            } else {
                (i16::from(x), i16::from(y))
            };
            mask & for_offset(x, y)
        }),
        MovementCapability::TwoStep { first, second } => {
            // An envelope of BOTH legs is deliberately conservative: any legal
            // path is nondecreasing, so a reset on either leg cannot be undone.
            nondecreasing_coordinates(first, color) & nondecreasing_coordinates(second, color)
        }
        MovementCapability::FreeEagleMultiMove { .. } => 0,
    }
}

fn compute_coordinates(piece: &Piece) -> u8 {
    MovementConfig::for_piece(piece)
        .capabilities
        .iter()
        .fold(u8::MAX, |mask, cap| {
            mask & nondecreasing_coordinates(cap, piece.color)
        })
}

fn progress_coordinates(piece: &Piece) -> u8 {
    // MovementConfig::for_piece currently has two state-dependent exceptions:
    // promoted RainDragon and Whale originating from ReverseChariot. The audit
    // test visits every promotion chain and checks cached vs uncached results.
    const PIECE_TYPES: usize = PieceType::SwordGeneral as usize + 1;
    static CACHE: [OnceLock<u8>; PIECE_TYPES * 4] = [const { OnceLock::new() }; PIECE_TYPES * 4];
    let special = (piece.piece_type == PieceType::RainDragon && piece.is_promoted)
        || (piece.piece_type == PieceType::Whale
            && piece.base_piece_type == Some(PieceType::ReverseChariot));
    let index = piece.piece_type as usize * 4
        + usize::from(piece.color == Color::White) * 2
        + usize::from(special);
    *CACHE[index].get_or_init(|| compute_coordinates(piece))
}

/// True only if this displacement increases a coordinate the piece can never
/// decrease without changing its movement type. Ignores occupancy: a temporary
/// blocker must not turn otherwise reversible maneuvering into progress.
/// Captures and actual promotions reset the counter separately in GameState.
pub fn move_is_directionally_irreversible(piece: &Piece, from: Position, to: Position) -> bool {
    let mask = progress_coordinates(piece);
    if mask == 0 {
        return false;
    }
    let dx = i16::from(to.file()) - i16::from(from.file());
    let dy = i16::from(to.rank()) - i16::from(from.rank());
    COORDINATES
        .iter()
        .enumerate()
        .any(|(i, &(a, b))| mask & (1 << i) != 0 && a * dx + b * dy > 0)
}

/// True if any leg advances a coordinate that every possible leg preserves or
/// increases. A closed quiet path therefore never resets the progress clock.
pub fn path_has_irreversible_leg(piece: &Piece, path: &[Position]) -> bool {
    path.windows(2)
        .any(|leg| move_is_directionally_irreversible(piece, leg[0], leg[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::{GameState, Move};

    #[test]
    fn cached_coordinates_cover_every_promotion_origin_and_color() {
        let from = Position::new(17, 17).unwrap();
        let mut seen = std::collections::HashSet::new();
        for &kind in crate::eval::ALL_PIECE_TYPES {
            for color in [Color::Black, Color::White] {
                let mut piece = Piece::new(kind, color, from);
                loop {
                    if !seen.insert((
                        piece.piece_type,
                        piece.color,
                        piece.is_promoted,
                        piece.base_piece_type,
                    )) {
                        break;
                    }
                    assert_eq!(
                        progress_coordinates(&piece),
                        compute_coordinates(&piece),
                        "{piece:?}"
                    );
                    if piece.piece_type.promotes_to().is_none() {
                        break;
                    }
                    piece.promote();
                }
            }
        }
        println!(
            "Audited {} piece forms including both colors and every promotion origin",
            seen.len()
        );
    }
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
