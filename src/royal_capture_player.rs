use crate::game_state::{GameState, Move};
use rand::Rng;

pub struct RoyalCapturePlayer;

impl RoyalCapturePlayer {
    /// Make a random legal move from the current position
    /// Prioritizes moves that capture royal pieces (King or Crown Prince)
    pub fn make_move(game_state: &GameState) -> Option<Move> {
        let legal_moves = game_state.generate_legal_moves();
        if legal_moves.is_empty() {
            return None;
        }
        
        let current_color = game_state.get_current_turn();
        let opponent_color = current_color.opposite();
        
        // Filter moves that capture royal pieces
        let royal_capture_moves: Vec<&Move> = legal_moves
            .iter()
            .filter(|mv| {
                // Check if the destination square has an opponent's royal piece
                if let Some(piece) = game_state.get_board().get_piece(mv.to) {
                    piece.color == opponent_color && piece.piece_type.is_royal()
                } else {
                    false
                }
            })
            .collect();
        
        let mut rng = rand::thread_rng();
        
        // If there are moves that capture royal pieces, randomly choose from those
        if !royal_capture_moves.is_empty() {
            let index = rng.gen_range(0..royal_capture_moves.len());
            return Some(royal_capture_moves[index].clone());
        }
        
        // Otherwise, randomly choose from all legal moves
        let index = rng.gen_range(0..legal_moves.len());
        Some(legal_moves[index].clone())
    }
}

