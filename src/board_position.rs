//! Compact serializable board snapshot for start pools and game records.

use crate::game_state::GameState;
use crate::piece::{Color, Piece};
use serde::{Deserialize, Serialize};

/// Engine-native position snapshot (not GUI string DTOs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPosition {
    pub pieces: Vec<Piece>,
    pub turn: Color,
    #[serde(default)]
    pub draw_counter: u32,
}

impl BoardPosition {
    pub fn from_state(state: &GameState) -> Self {
        let board = state.get_board();
        let mut pieces = Vec::with_capacity(
            board.get_pieces_by_color(Color::Black).len()
                + board.get_pieces_by_color(Color::White).len(),
        );
        for color in [Color::Black, Color::White] {
            pieces.extend(board.get_pieces_by_color(color).iter().copied());
        }
        Self {
            pieces,
            turn: state.get_current_turn(),
            draw_counter: state.get_turns_without_capture_or_promotion(),
        }
    }

    pub fn to_state(&self) -> GameState {
        let mut state = GameState::new();
        state.clear_board();
        for piece in &self.pieces {
            state.place_piece(*piece);
        }
        state.set_current_turn(self.turn);
        state.set_turns_without_capture_or_promotion(self.draw_counter);
        state.recompute_hash();
        state.reset_rep_history();
        state
    }

    pub fn save_path(&self, path: &std::path::Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize BoardPosition: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
    }

    pub fn load_path(path: &std::path::Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse BoardPosition {}: {}", path.display(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_round_trip() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let pos = BoardPosition::from_state(&state);
        let back = pos.to_state();
        assert_eq!(back.get_current_turn(), state.get_current_turn());
        assert_eq!(
            back.get_turns_without_capture_or_promotion(),
            state.get_turns_without_capture_or_promotion()
        );
        let b1 = state.get_board();
        let b2 = back.get_board();
        assert_eq!(
            b1.get_pieces_by_color(Color::Black).len(),
            b2.get_pieces_by_color(Color::Black).len()
        );
        assert_eq!(
            b1.get_pieces_by_color(Color::White).len(),
            b2.get_pieces_by_color(Color::White).len()
        );
        for color in [Color::Black, Color::White] {
            for p in b1.get_pieces_by_color(color) {
                let q = b2.get_piece(p.position).expect("piece present");
                assert_eq!(q.piece_type, p.piece_type);
                assert_eq!(q.color, p.color);
                assert_eq!(q.is_promoted, p.is_promoted);
            }
        }
    }
}
