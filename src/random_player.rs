use crate::game_state::{GameState, Move};
use rand::Rng;

pub struct RandomPlayer;

impl RandomPlayer {
    /// Make a random legal move from the current position
    pub fn make_move(game_state: &GameState) -> Option<Move> {
        let legal_moves = game_state.generate_legal_moves();
        if legal_moves.is_empty() {
            return None;
        }
        
        let mut rng = rand::thread_rng();
        let index = rng.gen_range(0..legal_moves.len());
        Some(legal_moves[index].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;

    #[test]
    fn test_random_player_returns_move() {
        let mut state = GameState::new();
        state.setup_initial_position();
        
        let mv = RandomPlayer::make_move(&state);
        assert!(mv.is_some());
    }

    #[test]
    fn test_random_player_returns_none_when_no_moves() {
        let state = GameState::new(); // Empty board
        let mv = RandomPlayer::make_move(&state);
        assert!(mv.is_none());
    }
}

