//! Incremental Zobrist keys for search TT.
//!
//! Keys are deterministic splitmix64 hashes of packed piece/side/draw identities
//! (no giant tables). Suitable for transposition keys, not cryptography.

use crate::game_state::GameState;
use crate::piece::{Color, Piece};

const PIECE_SEED: u64 = 0xC6A4_A793_5BD1_E995;
const SIDE_SEED: u64 = 0xA076_1D64_78BD_642F;
const DRAW_SEED: u64 = 0xE703_7ED1_A0B4_28DB;

#[inline]
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Zobrist key for a piece on its current square.
#[inline]
pub fn piece_key(piece: &Piece) -> u64 {
    let sq = (piece.position.file as u64) | ((piece.position.rank as u64) << 8);
    let id = (piece.piece_type as u64)
        | ((piece.color as u64) << 16)
        | ((piece.is_promoted as u64) << 17)
        | (sq << 18);
    splitmix64(id ^ PIECE_SEED)
}

#[inline]
pub fn side_key(turn: Color) -> u64 {
    match turn {
        Color::Black => splitmix64(SIDE_SEED),
        Color::White => splitmix64(SIDE_SEED ^ 1),
    }
}

#[inline]
pub fn draw_key(turns: u32) -> u64 {
    splitmix64(DRAW_SEED.wrapping_add(turns as u64))
}

/// Full-board Zobrist (source of truth for init / tests).
pub fn compute(state: &GameState) -> u64 {
    let mut h = side_key(state.get_current_turn());
    h ^= draw_key(state.get_turns_without_capture_or_promotion());
    let board = state.get_board();
    for color in [Color::Black, Color::White] {
        for p in board.pieces_by_color(color) {
            h ^= piece_key(p);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;
    use crate::piece::{Color, Piece, PieceType};
    use crate::position::Position;

    #[test]
    fn empty_and_opening_hashes_stable() {
        let empty = GameState::new();
        assert_eq!(empty.hash(), compute(&empty));
        let mut opening = GameState::new();
        opening.setup_initial_position();
        assert_eq!(opening.hash(), compute(&opening));
        assert_ne!(empty.hash(), opening.hash());
    }

    #[test]
    fn make_unmake_restores_hash() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let before = state.hash();
        let moves = state.generate_legal_moves();
        let mv = moves[0].clone();
        let undo = state.make_move_for_search(mv).expect("legal");
        assert_eq!(state.hash(), compute(&state));
        assert_ne!(state.hash(), before);
        state.unmake_move_for_search(undo);
        assert_eq!(state.hash(), before);
        assert_eq!(state.hash(), compute(&state));
    }

    #[test]
    fn capture_and_quiet_match_recompute() {
        let mut state = GameState::new();
        state.clear_board();
        state.place_piece(Piece::new(
            PieceType::Rook,
            Color::Black,
            Position::new(5, 5).unwrap(),
        ));
        state.place_piece(Piece::new(
            PieceType::Pawn,
            Color::White,
            Position::new(5, 10).unwrap(),
        ));
        state.set_current_turn(Color::Black);
        assert_eq!(state.hash(), compute(&state));

        let moves = state.generate_legal_moves();
        let capture = moves
            .iter()
            .find(|m| m.to == Position::new(5, 10).unwrap())
            .cloned()
            .expect("rook takes pawn");
        let undo = state.make_move_for_search(capture).unwrap();
        assert_eq!(state.hash(), compute(&state));
        state.unmake_move_for_search(undo);
        assert_eq!(state.hash(), compute(&state));
    }

    #[test]
    fn side_to_move_flip_updates_hash() {
        let mut state = GameState::new();
        let h0 = state.hash();
        state.set_current_turn(Color::White);
        assert_ne!(state.hash(), h0);
        assert_eq!(state.hash(), compute(&state));
        state.set_current_turn(Color::Black);
        assert_eq!(state.hash(), h0);
    }
}
