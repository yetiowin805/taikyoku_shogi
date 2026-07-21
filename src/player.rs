use crate::game_state::{GameState, Move};
use crate::minimal_intelligence_player::MinimalIntelligencePlayer;
use crate::random_player::RandomPlayer;
use crate::royal_capture_player::RoyalCapturePlayer;

/// Common interface for selecting a move from a position.
pub trait Player {
    fn name(&self) -> &'static str;
    fn choose_move(&self, state: &GameState) -> Option<Move>;
}

impl Player for RandomPlayer {
    fn name(&self) -> &'static str {
        "random"
    }

    fn choose_move(&self, state: &GameState) -> Option<Move> {
        RandomPlayer::make_move(state)
    }
}

impl Player for MinimalIntelligencePlayer {
    fn name(&self) -> &'static str {
        "mi"
    }

    fn choose_move(&self, state: &GameState) -> Option<Move> {
        MinimalIntelligencePlayer::make_move(state)
    }
}

impl Player for RoyalCapturePlayer {
    fn name(&self) -> &'static str {
        "royal"
    }

    fn choose_move(&self, state: &GameState) -> Option<Move> {
        RoyalCapturePlayer::make_move(state)
    }
}

/// Resolve a player by CLI name (`mi` / `heuristic`, `random`, `royal`).
pub fn player_by_name(name: &str) -> Result<Box<dyn Player>, String> {
    match name {
        "mi" | "heuristic" => Ok(Box::new(MinimalIntelligencePlayer)),
        "random" => Ok(Box::new(RandomPlayer)),
        "royal" => Ok(Box::new(RoyalCapturePlayer)),
        other => Err(format!(
            "Unknown player '{}'. Use mi, random, or royal",
            other
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::GameState;

    #[test]
    fn test_player_by_name_and_choose_move() {
        let mut state = GameState::new();
        state.setup_initial_position();

        let player = player_by_name("mi").unwrap();
        assert_eq!(player.name(), "mi");
        assert!(player.choose_move(&state).is_some());

        let player = player_by_name("random").unwrap();
        assert!(player.choose_move(&state).is_some());

        let player = player_by_name("royal").unwrap();
        assert!(player.choose_move(&state).is_some());
    }
}
