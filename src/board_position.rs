//! Compact serializable board snapshot for start pools and game records.

use crate::game_state::GameState;
use crate::piece::{Color, Piece};
use serde::{Deserialize, Serialize};

/// Engine-native position snapshot (not GUI string DTOs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "BoardPositionData")]
pub struct BoardPosition {
    pub pieces: Vec<Piece>,
    pub turn: Color,
    #[serde(default)]
    pub draw_counter: u32,
}

#[derive(Deserialize)]
struct BoardPositionData {
    pieces: Vec<Piece>,
    turn: Color,
    #[serde(default)]
    draw_counter: u32,
}

impl TryFrom<BoardPositionData> for BoardPosition {
    type Error = String;

    fn try_from(value: BoardPositionData) -> Result<Self, Self::Error> {
        let position = Self {
            pieces: value.pieces,
            turn: value.turn,
            draw_counter: value.draw_counter,
        };
        position.validate()?;
        Ok(position)
    }
}

impl BoardPosition {
    pub fn validate(&self) -> Result<(), String> {
        if self.draw_counter > crate::game_state::PROGRESS_DRAW_LIMIT {
            return Err(format!(
                "draw counter {} exceeds progress draw limit",
                self.draw_counter
            ));
        }
        let mut occupied = [false; 1296];
        for piece in &self.pieces {
            if std::mem::replace(&mut occupied[piece.position.to_index()], true) {
                return Err(format!(
                    "duplicate square: {},{}",
                    piece.position.file(),
                    piece.position.rank()
                ));
            }
            if let Some(base) = piece.base_piece_type {
                if !piece.is_promoted || base.promotes_to() != Some(piece.piece_type) {
                    return Err(format!(
                        "inconsistent promotion metadata at {},{}",
                        piece.position.file(),
                        piece.position.rank()
                    ));
                }
            }
        }
        Ok(())
    }

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

    pub fn to_state(&self) -> Result<GameState, String> {
        self.validate()?;
        let mut state = GameState::new();
        state.clear_board();
        for piece in &self.pieces {
            state.place_piece(*piece);
        }
        state.set_current_turn(self.turn);
        state.set_turns_without_capture_or_promotion(self.draw_counter);
        state.recompute_hash();
        state.reset_rep_history();
        Ok(state)
    }

    pub fn save_path(&self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
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
    use crate::piece::PieceType;
    use crate::position::Position;

    #[test]
    fn rejects_duplicate_squares_and_invalid_metadata_before_conversion() {
        let piece = Piece::new(PieceType::Pawn, Color::Black, Position::new(0, 1).unwrap());
        let mut position = BoardPosition {
            pieces: vec![piece, piece],
            turn: Color::Black,
            draw_counter: 0,
        };
        assert!(position.to_state().is_err());
        assert!(
            serde_json::from_value::<BoardPosition>(serde_json::to_value(&position).unwrap())
                .is_err()
        );
        position.pieces.pop();
        position.pieces[0].base_piece_type = Some(PieceType::Rook);
        assert!(position.to_state().is_err());
        position.pieces[0] = piece;
        position.draw_counter = u32::MAX;
        assert!(position.to_state().is_err());
    }

    #[test]
    fn legacy_promoted_snapshot_without_origin_remains_readable() {
        let json = serde_json::json!({
            "pieces": [{"piece_type": "Whale", "color": "Black",
                "position": {"file": 0, "rank": 1}, "is_promoted": true}],
            "turn": "Black"
        });
        let position: BoardPosition = serde_json::from_value(json).unwrap();
        assert!(position.to_state().is_ok());
    }

    #[test]
    fn invalid_coordinate_cannot_alias_another_snapshot_square() {
        let json = serde_json::json!({
            "pieces": [{"piece_type": "Pawn", "color": "Black",
                "position": {"file": 36, "rank": 0}, "is_promoted": false}],
            "turn": "Black"
        });
        assert!(serde_json::from_value::<BoardPosition>(json).is_err());
    }

    #[test]
    fn opening_round_trip() {
        let mut state = GameState::new();
        state.setup_initial_position();
        let pos = BoardPosition::from_state(&state);
        let back = pos.to_state().unwrap();
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
