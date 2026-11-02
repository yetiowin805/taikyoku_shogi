//! Small repeated-batch probes for the non-search state audit.
//! Usage: cargo run --release --example state_speed_probe -- [positions.json]
//! Optional JSON input is an array of BoardPosition values.

use std::hint::black_box;
use std::time::Instant;
use taikyoku_shogi::board_position::BoardPosition;
use taikyoku_shogi::game_history::GameHistory;
use taikyoku_shogi::game_state::{GameState, Move, MoveData};
use taikyoku_shogi::move_simulation::{simulate_move, BoardLike};
use taikyoku_shogi::notation::{tsfen_decode, tsfen_encode};
use taikyoku_shogi::piece::{Color, Piece, PieceType};
use taikyoku_shogi::position::Position;

fn ns_per_call(mut f: impl FnMut(), count: usize) -> f64 {
    for _ in 0..32 {
        f();
    }
    let start = Instant::now();
    for _ in 0..count {
        f();
    }
    start.elapsed().as_nanos() as f64 / count as f64
}

fn compare(name: &str, count: usize, mut old: impl FnMut(), mut new: impl FnMut()) {
    let mut old_ns = Vec::new();
    let mut new_ns = Vec::new();
    for round in 0..7 {
        if round % 2 == 0 {
            old_ns.push(ns_per_call(&mut old, count));
            new_ns.push(ns_per_call(&mut new, count));
        } else {
            new_ns.push(ns_per_call(&mut new, count));
            old_ns.push(ns_per_call(&mut old, count));
        }
    }
    println!(
        "{}",
        serde_json::json!({"probe":name,"count_per_round":count,
        "rounds":7,"old_ns":old_ns,"new_ns":new_ns})
    );
}

fn old_draw(state: &GameState) -> bool {
    let black = state.get_board().get_pieces_by_color(Color::Black);
    let white = state.get_board().get_pieces_by_color(Color::White);
    let allowed = |piece: &Piece| {
        matches!(
            piece.piece_type,
            PieceType::King
                | PieceType::CrownPrince
                | PieceType::GreatGeneral
                | PieceType::DrunkenElephant
                | PieceType::FreeKing
        )
    };
    !black.is_empty() && !white.is_empty() && black.iter().all(allowed) && white.iter().all(allowed)
}

fn old_snapshot(state: &GameState) -> BoardPosition {
    let board = state.get_board();
    let mut pieces = Vec::with_capacity(
        board.get_pieces_by_color(Color::Black).len()
            + board.get_pieces_by_color(Color::White).len(),
    );
    for color in [Color::Black, Color::White] {
        pieces.extend(board.get_pieces_by_color(color).iter().copied());
    }
    BoardPosition {
        pieces,
        turn: state.get_current_turn(),
        draw_counter: state.get_turns_without_capture_or_promotion(),
    }
}

fn old_load(position: &BoardPosition) -> GameState {
    position.validate().unwrap();
    let mut state = GameState::new();
    state.clear_board();
    for piece in &position.pieces {
        state.place_piece(*piece);
    }
    state.set_current_turn(position.turn);
    state.set_turns_without_capture_or_promotion(position.draw_counter);
    state.recompute_hash();
    state.reset_rep_history();
    state
}

fn packed_snapshot(position: &BoardPosition) -> Vec<u8> {
    let mut out = Vec::with_capacity(7 + 8 * position.pieces.len());
    out.extend_from_slice(&(position.pieces.len() as u16).to_le_bytes());
    out.push(position.turn as u8);
    out.extend_from_slice(&position.draw_counter.to_le_bytes());
    for p in &position.pieces {
        out.extend_from_slice(&(p.piece_type as u16).to_le_bytes());
        out.extend_from_slice(
            &p.base_piece_type
                .map_or(u16::MAX, |base| base as u16)
                .to_le_bytes(),
        );
        out.extend_from_slice(&[
            p.position.file(),
            p.position.rank(),
            p.color as u8,
            p.is_promoted as u8,
        ]);
    }
    out
}

fn checkpoint_probe() {
    let mut root = GameState::new();
    root.place_piece(Piece::new(
        PieceType::King,
        Color::Black,
        Position::new(0, 0).unwrap(),
    ));
    root.place_piece(Piece::new(
        PieceType::King,
        Color::White,
        Position::new(35, 35).unwrap(),
    ));
    let cycle = [
        (0, 0, 0, 1),
        (35, 35, 35, 34),
        (0, 1, 0, 0),
        (35, 34, 35, 35),
    ];
    for len in [64, 512] {
        let moves: Vec<_> = (0..len)
            .map(|i| {
                let (ff, fr, tf, tr) = cycle[i % 4];
                Move::new(
                    Position::new(ff, fr).unwrap(),
                    Position::new(tf, tr).unwrap(),
                )
            })
            .collect();
        let mut checkpoint = root.clone();
        for mv in &moves[..len - 32] {
            checkpoint.make_move(mv.clone());
        }
        let replay = |base: &GameState, tail: &[Move]| {
            let mut state = base.clone();
            for mv in tail {
                state.make_move(mv.clone());
            }
            state
        };
        let all = replay(&root, &moves);
        let tail = replay(&checkpoint, &moves[len - 32..]);
        assert_eq!(all.hash(), tail.hash());
        assert_eq!(all.repetition_count(), tail.repetition_count());
        assert_eq!(all.get_move_history().len(), tail.get_move_history().len());
        compare(
            &format!("C03_checkpoint_{len}_plies_last32"),
            100,
            || {
                black_box(replay(black_box(&root), black_box(&moves)));
            },
            || {
                black_box(replay(
                    black_box(&checkpoint),
                    black_box(&moves[len - 32..]),
                ));
            },
        );
    }
}

fn overlay_probe(state: &GameState, label: &str) {
    let moves = state.generate_legal_moves();
    let Some(mv) = moves
        .iter()
        .find(|mv| !mv.is_free_eagle() && !mv.is_two_step())
    else {
        return;
    };
    let original = state.get_board().get_piece(mv.from).unwrap();
    let overlay = simulate_move(state.get_board(), mv, &original);
    for color in [Color::Black, Color::White] {
        let delta = overlay.delta();
        let moved = delta.piece_moved.and_then(|(_, to, piece)| {
            (piece.color == color)
                .then(|| overlay.get_piece(to))
                .flatten()
        });
        let borrowed_count = |target: Position| {
            state
                .get_board()
                .pieces_by_color(color)
                .iter()
                .copied()
                .filter(|piece| {
                    delta.piece_moved.is_none_or(|(from, _, mover)| {
                        mover.color != color || piece.position != from
                    })
                })
                .chain(moved)
                .filter(|piece| {
                    !delta
                        .pieces_removed
                        .iter()
                        .any(|(square, victim)| victim.color == color && piece.position == *square)
                })
                .filter(|piece| {
                    taikyoku_shogi::attack_utils::should_check_piece_for_target_position(
                        piece, target, false,
                    )
                })
                .count()
        };
        let materialized_count = |target: Position| {
            overlay
                .get_pieces_by_color(color)
                .iter()
                .filter(|piece| {
                    taikyoku_shogi::attack_utils::should_check_piece_for_target_position(
                        piece, target, false,
                    )
                })
                .count()
        };
        for square in (0..1296).step_by(13) {
            let target = Position::from_index(square).unwrap();
            assert_eq!(materialized_count(target), borrowed_count(target));
        }
        let mut old_query = 0usize;
        let mut new_query = 0usize;
        compare(
            &format!("R19_virtual_army_candidate_count_{label}_{color:?}"),
            10_000,
            || {
                old_query = (old_query + 313) % 1296;
                black_box(materialized_count(black_box(
                    Position::from_index(old_query).unwrap(),
                )));
            },
            || {
                new_query = (new_query + 313) % 1296;
                black_box(borrowed_count(black_box(
                    Position::from_index(new_query).unwrap(),
                )));
            },
        );
    }
}

fn main() {
    let mut opening = GameState::new();
    opening.setup_initial_position();
    let mut positions = vec![BoardPosition::from_state(&opening)];
    if let Some(path) = std::env::args().nth(1) {
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let array = value
            .as_array()
            .unwrap_or_else(|| value["positions"].as_array().unwrap());
        for entry in array {
            let snapshot = entry.get("position").unwrap_or(entry);
            let position = if let Some(text) = snapshot.as_str().or_else(|| entry["tsfen"].as_str())
            {
                tsfen_decode(text).unwrap()
            } else {
                serde_json::from_value(snapshot.clone()).unwrap()
            };
            positions.push(position);
        }
    }
    for (index, position) in positions.iter().enumerate() {
        let state = position.to_state().unwrap();
        assert_eq!(old_draw(&state), state.is_draw_by_insufficient_material());
        assert_eq!(old_snapshot(&state), BoardPosition::from_state(&state));
        let old_state = old_load(position);
        assert_eq!(old_state.hash(), state.hash());
        assert_eq!(
            tsfen_encode(&BoardPosition::from_state(&old_state)),
            tsfen_encode(position)
        );
        let label = format!("position_{index}_pieces_{}", position.pieces.len());
        overlay_probe(&state, &label);
        compare(
            &format!("R01_{label}"),
            20_000,
            || {
                black_box(old_draw(black_box(&state)));
            },
            || {
                black_box(black_box(&state).is_draw_by_insufficient_material());
            },
        );
        compare(
            &format!("R02_{label}"),
            2_000,
            || {
                black_box(old_snapshot(black_box(&state)));
            },
            || {
                black_box(BoardPosition::from_state(black_box(&state)));
            },
        );
        compare(
            &format!("S01_{label}"),
            100,
            || {
                black_box(old_load(black_box(position)));
            },
            || {
                black_box(black_box(position).to_state().unwrap());
            },
        );
        compare(
            &format!("S12_full_hash_lower_bound_{label}"),
            20_000,
            || {
                black_box(taikyoku_shogi::zobrist::compute(black_box(&state)));
            },
            || {
                black_box(black_box(&state).hash());
            },
        );
        let keys: Vec<_> = position
            .pieces
            .iter()
            .map(taikyoku_shogi::zobrist::piece_key)
            .collect();
        compare(
            &format!("S16_existing_piece_key_cache_bound_{label}"),
            20_000,
            || {
                black_box(
                    black_box(&position.pieces)
                        .iter()
                        .fold(0u64, |h, p| h ^ taikyoku_shogi::zobrist::piece_key(p)),
                );
            },
            || {
                black_box(black_box(&keys).iter().fold(0u64, |h, key| h ^ key));
            },
        );
        compare(
            &format!("S18_snapshot_write_only_{label}"),
            100,
            || {
                black_box(tsfen_encode(black_box(position)));
            },
            || {
                black_box(packed_snapshot(black_box(position)));
            },
        );
        println!(
            "{}",
            serde_json::json!({"probe":format!("S18_size_{label}"),
            "tsfen_bytes":tsfen_encode(position).len(),
            "json_bytes":serde_json::to_vec(position).unwrap().len(),
            "packed_prototype_bytes":packed_snapshot(position).len(),
            "decode_validated":false})
        );
    }
    let from = Position::new(18, 18).unwrap();
    let mut eagle = GameState::new();
    eagle.place_piece(Piece::new(PieceType::FreeEagle, Color::Black, from));
    eagle.place_piece(Piece::new(
        PieceType::Pawn,
        Color::White,
        Position::new(19, 19).unwrap(),
    ));
    let mv = Move::new(from, Position::new(19, 19).unwrap());
    let mut old_eagle = eagle.clone();
    compare(
        "S09_FE_standard_make_unmake",
        20_000,
        || {
            let mut normalized = mv.clone();
            let piece = old_eagle.get_board().get_piece(from).unwrap();
            let routes = old_eagle.generate_legal_moves_for_pieces(&[piece]);
            if let Some(path) = routes
                .iter()
                .find(|route| route.to == mv.to)
                .and_then(Move::free_eagle_path)
            {
                normalized.data = MoveData::FreeEagle { path: path.into() };
            }
            let undo = old_eagle
                .make_move_for_search(black_box(normalized))
                .unwrap();
            old_eagle.unmake_move_for_search(undo);
        },
        || {
            let undo = eagle.make_move_for_search(black_box(mv.clone())).unwrap();
            eagle.unmake_move_for_search(undo);
        },
    );
    for len in [10, 1000, 10_000] {
        let record = GameHistory::move_to_record(&mv, Color::Black, 1);
        let timeline = vec![record; len];
        compare(
            &format!("C02_timeline_last_next_len_{len}"),
            1_000,
            || {
                let owned = black_box(&timeline).clone();
                black_box((owned.get(len / 2 - 1), owned.get(len / 2)));
            },
            || {
                black_box((
                    black_box(&timeline).get(len / 2 - 1),
                    black_box(&timeline).get(len / 2),
                ));
            },
        );
    }
    let kings: Vec<_> = (0..804)
        .map(|i| {
            Piece::new(
                PieceType::King,
                if i % 2 == 0 {
                    Color::Black
                } else {
                    Color::White
                },
                Position::from_index(i).unwrap(),
            )
        })
        .collect();
    let king_state = BoardPosition {
        pieces: kings,
        turn: Color::Black,
        draw_counter: 0,
    }
    .to_state()
    .unwrap();
    compare(
        "S17_all_draw_class_804_query_only",
        20_000,
        || {
            black_box(black_box(&king_state).is_draw_by_insufficient_material());
        },
        || {
            black_box(black_box(0usize) == 0);
        },
    );
    checkpoint_probe();
}
