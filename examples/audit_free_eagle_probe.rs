//! Endpoint-equivalent generated FreeEagle representations, no engine changes.
use std::hint::black_box;
use std::time::Instant;

use serde_json::json;
use taikyoku_shogi::game_state::{GameState, MoveData};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

fn main() {
    let mut initial = GameState::new();
    initial.place_piece(Piece::new(PieceType::FreeEagle, Color::Black, p(18, 18)));
    initial.place_piece(Piece::new(PieceType::Pawn, Color::White, p(19, 19)));
    let generated = initial.generate_legal_moves();
    let path = generated
        .iter()
        .find(|mv| mv.to == p(19, 19) && matches!(mv.data, MoveData::FreeEagle { .. }))
        .unwrap()
        .clone();
    let standard = generated
        .iter()
        .find(|mv| mv.to == path.to && matches!(mv.data, MoveData::Standard))
        .unwrap()
        .clone();
    let mut path_state = initial.clone();
    let mut standard_state = initial.clone();
    path_state.make_move(path.clone());
    standard_state.make_move(standard.clone());
    for index in 0..1296 {
        let square = Position::from_index(index).unwrap();
        assert_eq!(
            path_state.get_board().get_piece(square),
            standard_state.get_board().get_piece(square)
        );
    }
    assert_eq!(path_state.hash(), standard_state.hash());
    assert_eq!(
        path_state.get_turns_without_capture_or_promotion(),
        standard_state.get_turns_without_capture_or_promotion()
    );
    for method in 0..4 {
        for _ in 0..100 {
            let mv = if method % 2 == 0 { &standard } else { &path };
            if method < 2 {
                let undo = initial.make_move_for_search(mv.clone()).unwrap();
                initial.unmake_move_for_search(undo);
            } else {
                let mut child = initial.clone();
                child.make_move(mv.clone());
                black_box(child);
            }
        }
    }
    let repetitions = 10_000;
    let mut rounds = Vec::new();
    for round in 0..5 {
        let mut timings = [0.; 4];
        let order = if round % 2 == 0 {
            [0, 1, 2, 3]
        } else {
            [3, 2, 1, 0]
        };
        for method in order {
            let mv = if method % 2 == 0 { &standard } else { &path };
            let start = Instant::now();
            for _ in 0..repetitions {
                if method < 2 {
                    let undo = initial.make_move_for_search(black_box(mv.clone())).unwrap();
                    black_box(initial.hash());
                    initial.unmake_move_for_search(undo);
                } else {
                    let mut child = black_box(&initial).clone();
                    child.make_move(black_box(mv.clone()));
                    black_box(child);
                }
            }
            timings[method] = start.elapsed().as_nanos() as f64 / f64::from(repetitions);
        }
        rounds.push(json!({"round":round,
            "trusted_standard_make_unmake_ns":timings[0],
            "trusted_path_make_unmake_ns":timings[1],
            "clone_and_validated_standard_make_ns":timings[2],
            "clone_and_validated_path_make_ns":timings[3]}));
    }
    assert_eq!(initial.hash(), taikyoku_shogi::zobrist::compute(&initial));
    assert_eq!(initial.get_current_turn(), Color::Black);
    assert_eq!(
        initial.get_board().get_piece(p(19, 19)).unwrap().color,
        Color::White
    );
    println!(
        "{}",
        json!({"probe":"free_eagle_representation_apply", "standard":standard,"path":path,
        "generated_moves":generated.len(),"repetitions_per_round":repetitions,"warmup_per_method":100,
        "order":"alternating forward/reverse", "correctness":"all squares, hash, draw counter equal; final undo hash canonical",
        "scope":"trusted core make/unmake and clone plus checked core apply; no search executed", "rounds":rounds})
    );
}
