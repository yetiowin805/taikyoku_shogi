//! Emit only selected replay positions; never export whole games or model weights.
use std::{collections::BTreeSet, path::Path};
use taikyoku_shogi::{
    board_position::BoardPosition,
    game_history::GameHistory,
    game_state::GameState,
    notation::tsfen_encode,
    training::record::{GameRecordV2, GameStart},
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 3, "GAME ZERO_BASED_PLIES_CSV");
    let wanted: BTreeSet<usize> = args[2].split(',').map(|p| p.parse().unwrap()).collect();
    let record = GameRecordV2::load_path(Path::new(&args[1])).unwrap();
    let last = *wanted.last().unwrap();
    assert!(last < record.moves.len());
    let mut state = match &record.start {
        GameStart::Opening => {
            let mut state = GameState::new();
            state.setup_initial_position();
            state
        }
        GameStart::Position { position } => position.to_state().unwrap(),
    };
    for ply in 0..=last {
        if wanted.contains(&ply) {
            let position = BoardPosition::from_state(&state);
            println!(
                "{}",
                serde_json::json!({
                    "source_game":args[1], "source_timestamp":record.timestamp,
                    "ply":ply, "pieces":position.pieces.len(),
                    "history_legs":state.get_move_history().len(),
                    "repetition_count":state.repetition_count(),
                    "tsfen":tsfen_encode(&position)
                })
            );
        }
        if ply < last {
            let next = &record.moves[ply];
            assert_eq!(state.get_current_turn(), next.color, "color at {ply}");
            state.make_move(GameHistory::record_to_move(next).unwrap());
            assert_ne!(state.get_current_turn(), next.color, "move at {ply}");
        }
    }
}
