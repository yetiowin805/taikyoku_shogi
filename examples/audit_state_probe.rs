//! Disposable non-search audit probes; no engine implementation changes.
use std::hint::black_box;
use std::time::Instant;

use serde_json::json;
use taikyoku_shogi::board::Board;
use taikyoku_shogi::board_position::BoardPosition;
use taikyoku_shogi::eval::{EvalInc, EvalWeights, ALL_PIECE_TYPES};
use taikyoku_shogi::game_history::{GameHistory, GameRecord};
use taikyoku_shogi::game_state::{GameState, Move, MoveData, SearchUndo};
use taikyoku_shogi::move_simulation::{move_to_delta, simulate_move, BoardLike};
use taikyoku_shogi::notation::{move_decode, move_encode};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

fn put(state: &mut GameState, kind: PieceType, color: Color, file: u8, rank: u8) {
    state.place_piece(Piece::new(kind, color, p(file, rank)));
}

fn same_board(left: &Board, right: &Board) -> bool {
    (0..1296).all(|index| {
        let position = Position::from_index(index).unwrap();
        left.get_piece(position) == right.get_piece(position)
    })
}

fn emit(value: serde_json::Value) {
    println!("{value}");
}

fn public_boundary_probes() {
    let mut early = GameState::new();
    put(&mut early, PieceType::Pawn, Color::Black, 8, 8);
    let forced = Move::new_with_promotion(p(8, 8), p(8, 9), true);
    let listed = early.generate_legal_moves().contains(&forced);
    let old_turn = early.get_current_turn();
    let result = early.make_move(forced);
    emit(json!({"probe":"outside_zone_promotion", "listed":listed,
        "applied":old_turn != early.get_current_turn(), "return_is_none":result.is_none(),
        "final_piece":early.get_board().get_piece(p(8, 9))}));

    let mut mandatory = GameState::new();
    put(&mut mandatory, PieceType::Pawn, Color::Black, 8, 34);
    let declined = Move::new(p(8, 34), p(8, 35));
    let listed = mandatory.generate_legal_moves().contains(&declined);
    mandatory.make_move(declined);
    emit(
        json!({"probe":"decline_mandatory_promotion", "listed":listed,
        "applied":mandatory.get_current_turn() == Color::White,
        "final_piece":mandatory.get_board().get_piece(p(8, 35))}),
    );

    let mut repeated = GameState::new();
    let mut promoted = Piece::new(PieceType::Pawn, Color::Black, p(8, 8));
    promoted.promote();
    repeated.place_piece(promoted);
    repeated.make_move(Move::new_with_promotion(p(8, 8), p(8, 9), true));
    emit(
        json!({"probe":"second_promotion", "applied":repeated.get_current_turn() == Color::White,
        "final_piece":repeated.get_board().get_piece(p(8, 9)),
        "snapshot_valid":BoardPosition::from_state(&repeated).validate().is_ok()}),
    );

    let mut fake_promotion = GameState::new();
    put(&mut fake_promotion, PieceType::King, Color::Black, 8, 8);
    fake_promotion.set_turns_without_capture_or_promotion(99);
    fake_promotion.make_move(Move::new_with_promotion(p(8, 8), p(8, 9), true));
    emit(json!({"probe":"unpromotable_flag_resets_progress",
        "applied":fake_promotion.get_current_turn() == Color::White,
        "draw_counter":fake_promotion.get_turns_without_capture_or_promotion(),
        "final_piece":fake_promotion.get_board().get_piece(p(8, 9))}));

    let mut stale = GameState::new();
    put(&mut stale, PieceType::Pawn, Color::Black, 8, 8);
    stale.get_board_mut().move_piece(p(8, 8), p(8, 9));
    emit(json!({"probe":"board_mutation_hash", "hash_matches":
        stale.hash() == taikyoku_shogi::zobrist::compute(&stale),
        "repetition_count":stale.repetition_count()}));

    let mut turn = GameState::new();
    turn.set_current_turn(Color::White);
    emit(json!({"probe":"set_turn_repetition_seed", "count":turn.repetition_count()}));

    let mut restored = GameState::new();
    put(&mut restored, PieceType::Pawn, Color::Black, 8, 8);
    let weights = EvalWeights::default();
    restored.ensure_eval_inc(&weights);
    restored.restore_position(Board::new(), Color::Black, 0);
    restored.ensure_eval_inc(&weights);
    emit(
        json!({"probe":"restore_position_cache", "cache_matches_board":
        restored.eval_inc() == Some(&EvalInc::from_board(restored.get_board(), &weights))}),
    );

    let mut eagle = GameState::new();
    put(&mut eagle, PieceType::FreeEagle, Color::Black, 18, 18);
    let moves = eagle.generate_legal_moves();
    let mut mismatches = 0;
    let mut board_mismatches = 0;
    let mut decoded_rejected = 0;
    let mut first = None;
    for mv in &moves {
        let token = move_encode(mv);
        let decoded = move_decode(&token).unwrap();
        if decoded != *mv {
            mismatches += 1;
            first.get_or_insert(json!({"original":mv, "token":token, "decoded":decoded}));
        }
        let mut original_state = eagle.clone();
        let mut decoded_state = eagle.clone();
        original_state.make_move(mv.clone());
        decoded_state.make_move(decoded);
        decoded_rejected += usize::from(decoded_state.get_current_turn() == Color::Black);
        board_mismatches += usize::from(!same_board(
            original_state.get_board(),
            decoded_state.get_board(),
        ));
    }
    emit(
        json!({"probe":"generated_free_eagle_tm1_round_trip", "moves":moves.len(),
        "move_mismatches":mismatches, "board_mismatches":board_mismatches,
        "decoded_rejected":decoded_rejected, "first":first}),
    );
    let mut history_cases = Vec::new();
    for length in [2, 3, 4, 5] {
        if let Some(mv) = moves.iter().find(|mv| {
            mv.free_eagle_path()
                .is_some_and(|path| path.len() == length)
        }) {
            let mut child = eagle.clone();
            child.make_move(mv.clone());
            history_cases.push(json!({"path_len":length,"history":child.get_move_history(),
                "history_len":child.get_move_history().len(),"move":mv}));
        }
    }
    emit(json!({"probe":"generated_free_eagle_record_history", "cases":history_cases}));

    let mut board = Board::new();
    let a = Piece::new(PieceType::King, Color::Black, p(1, 1));
    let b = Piece::new(PieceType::King, Color::White, p(6, 6));
    board.place_piece(a);
    board.place_piece(b);
    let first = simulate_move(&board, &Move::new(a.position, p(2, 2)), &a);
    let composed = first.compose(move_to_delta(&board, &Move::new(b.position, p(5, 5)), &b));
    emit(json!({"probe":"virtual_board_compose_distinct_movers",
        "first_origin_empty":composed.get_piece(a.position).is_none(),
        "first_destination_present":composed.get_piece(p(2, 2)).is_some(),
        "second_origin_empty":composed.get_piece(b.position).is_none(),
        "second_destination_present":composed.get_piece(p(5, 5)).is_some()}));

    let history = GameHistory::new("/tmp/audit-state-history");
    let mut malformed = GameHistory::move_to_record(&Move::new(p(0, 0), p(0, 1)), Color::Black, 1);
    malformed.from_file = 255;
    let record = GameRecord {
        timestamp: 1,
        moves: vec![malformed],
        result: None,
    };
    let checked_conversion_rejects = GameHistory::record_to_move(&record.moves[0]).is_err();
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let formatting_panics = std::panic::catch_unwind(|| history.format_game(&record)).is_err();
    std::panic::set_hook(panic_hook);
    emit(
        json!({"probe":"malformed_legacy_game_format", "checked_conversion_rejects":checked_conversion_rejects,
        "formatting_panics":formatting_panics}),
    );
}

fn generated_parity_probe() {
    let mut checked = 0;
    let mut generated = 0;
    let mut rejected = 0;
    let mut parity_failures = 0;
    let mut undo_failures = 0;
    let mut simulation_failures = 0;
    let mut first_failure = None;
    for &kind in ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                let mut state = GameState::new();
                let mut mover = Piece::new(kind, color, p(18, 18));
                if promoted {
                    if kind.promotes_to().is_none() {
                        continue;
                    }
                    mover.promote();
                }
                state.place_piece(mover);
                for (file, rank) in [(19, 19), (20, 18), (16, 18), (18, 16), (21, 21), (18, 22)] {
                    put(&mut state, PieceType::Pawn, color.opposite(), file, rank);
                }
                for (file, rank) in [(17, 17), (18, 20), (21, 18)] {
                    put(&mut state, PieceType::Pawn, color, file, rank);
                }
                state.set_current_turn(color);
                state.reset_rep_history();
                state.set_turns_without_capture_or_promotion(42);
                let moves = state.generate_legal_moves_for_pieces(&[mover]);
                generated += moves.len();
                let stride = (moves.len() / 16).max(1);
                for mv in moves.iter().step_by(stride).take(17) {
                    checked += 1;
                    let simulation = simulate_move(state.get_board(), mv, &mover);
                    let mut playing = state.clone();
                    playing.make_move(mv.clone());
                    let mut fast = state.clone();
                    let undo = fast.make_move_for_search(mv.clone());
                    if playing.get_current_turn() == color || undo.is_none() {
                        rejected += 1;
                        first_failure.get_or_insert(json!({"kind":kind,"promoted":promoted,
                            "color":color,"move":mv,"failure":"rejected"}));
                        continue;
                    }
                    if !same_board(playing.get_board(), fast.get_board())
                        || playing.hash() != fast.hash()
                        || playing.get_turns_without_capture_or_promotion()
                            != fast.get_turns_without_capture_or_promotion()
                        || fast.hash() != taikyoku_shogi::zobrist::compute(&fast)
                    {
                        parity_failures += 1;
                        first_failure.get_or_insert(json!({"kind":kind,"promoted":promoted,
                            "color":color,"move":mv,"failure":"public-fast-parity"}));
                    }
                    if (0..1296).any(|index| {
                        let square = Position::from_index(index).unwrap();
                        simulation.get_piece(square) != playing.get_board().get_piece(square)
                    }) {
                        simulation_failures += 1;
                        first_failure.get_or_insert(json!({"kind":kind,"promoted":promoted,
                            "color":color,"move":mv,"failure":"simulation-parity"}));
                    }
                    fast.unmake_move_for_search(undo.unwrap());
                    if !same_board(state.get_board(), fast.get_board())
                        || state.hash() != fast.hash()
                        || fast.repetition_count() != state.repetition_count()
                        || state.get_turns_without_capture_or_promotion()
                            != fast.get_turns_without_capture_or_promotion()
                        || state.get_current_turn() != fast.get_current_turn()
                    {
                        undo_failures += 1;
                    }
                }
            }
        }
    }
    emit(
        json!({"probe":"generated_make_unmake_simulation_parity", "generated":generated,
        "checked":checked,"rejected":rejected,"parity_failures":parity_failures,
        "simulation_failures":simulation_failures,"undo_failures":undo_failures,"first":first_failure}),
    );
    assert_eq!(rejected, 0, "generated moves must apply in both core paths");
    assert_eq!(
        parity_failures, 0,
        "core state/hash application parity failed"
    );
    assert_eq!(
        simulation_failures, 0,
        "virtual square simulation parity failed"
    );
    assert_eq!(
        undo_failures, 0,
        "core undo did not restore the prior state"
    );
}

fn bulk_load(position: &BoardPosition) -> GameState {
    position.validate().unwrap();
    let mut board = Board::new();
    for &piece in &position.pieces {
        board.place_piece(piece);
    }
    let mut state = GameState::new();
    state.restore_position(board, position.turn, position.draw_counter);
    state
}

fn load_benchmark() {
    let mut state = GameState::new();
    state.setup_initial_position();
    let position = BoardPosition::from_state(&state);
    let bulk = bulk_load(&position);
    assert!(same_board(state.get_board(), bulk.get_board()));
    assert_eq!(state.hash(), bulk.hash());
    for _ in 0..30 {
        black_box(position.to_state().unwrap());
        black_box(bulk_load(&position));
    }
    let reps = 300;
    let mut rounds = Vec::new();
    for round in 0..8 {
        let mut current_ns = 0.;
        let mut bulk_ns = 0.;
        for method in if round % 2 == 0 { [0, 1] } else { [1, 0] } {
            let start = Instant::now();
            for _ in 0..reps {
                if method == 0 {
                    black_box(black_box(&position).to_state().unwrap());
                } else {
                    black_box(bulk_load(black_box(&position)));
                }
            }
            let elapsed = start.elapsed().as_nanos() as f64 / f64::from(reps);
            if method == 0 {
                current_ns = elapsed;
            } else {
                bulk_ns = elapsed;
            }
        }
        rounds.push(
            json!({"round":round,"current_ns_per_load":current_ns,"bulk_ns_per_load":bulk_ns,
                "first":if round % 2 == 0 {"current"} else {"bulk"}}),
        );
    }
    emit(
        json!({"probe":"snapshot_bulk_load_benchmark","pieces":position.pieces.len(),
        "repetitions_per_round":reps,"warmup_per_method":30,"order":"alternating",
        "rounds":rounds,"correctness":"all squares and hash match"}),
    );
    emit(
        json!({"probe":"representation_sizes","move":std::mem::size_of::<Move>(),
        "move_data":std::mem::size_of::<MoveData>(),"piece":std::mem::size_of::<Piece>(),
        "search_undo":std::mem::size_of::<SearchUndo>(),"board":std::mem::size_of::<Board>()}),
    );
}

fn main() {
    public_boundary_probes();
    generated_parity_probe();
    load_benchmark();
}
