//! Rule oracle for range two-movers: empty corner, right-angle turn, one capture.
use super::*;
use crate::movement::{MovementCapability, MovementGenerator};
use std::collections::BTreeSet;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

// Intentionally independent of MovementGenerator and the production turn predicate.
fn walk(
    board: &Board,
    piece: Piece,
    from: Position,
    direction: (i8, i8),
    limit: i8,
) -> Vec<Position> {
    let mut out = Vec::new();
    for distance in 1..=limit {
        let Some(to) = from.offset(direction.0 * distance, direction.1 * distance) else {
            break;
        };
        match board.get_piece(to) {
            Some(occupant) => {
                if occupant.color != piece.color {
                    out.push(to);
                }
                break;
            }
            None => out.push(to),
        }
    }
    out
}

#[test]
fn only_the_four_range_two_mover_families_receive_the_restriction() {
    let mut kinds = BTreeSet::new();
    for &kind in crate::eval::ALL_PIECE_TYPES {
        let piece = Piece::new(kind, Color::Black, p(18, 18));
        if MovementConfig::for_piece(&piece)
            .capabilities
            .iter()
            .any(|cap| cap.is_range_two_step())
        {
            kinds.insert(format!("{kind:?}"));
        }
    }
    assert_eq!(
        kinds,
        ["Capricorn", "HookMover", "Peacock", "Tengu"]
            .map(String::from)
            .into_iter()
            .collect()
    );
}

#[test]
fn range_generation_and_attacks_match_an_independent_corner_walk() {
    for kind in [
        PieceType::HookMover,
        PieceType::Tengu,
        PieceType::Capricorn,
        PieceType::Peacock,
    ] {
        for color in [Color::Black, Color::White] {
            for origin in [p(18, 18), p(1, 1), p(34, 34)] {
                for layout in 0..3 {
                    let mut state = GameState::new();
                    state.set_current_turn(color);
                    let piece = Piece::new(kind, color, origin);
                    state.place_piece(piece);
                    if layout > 0 {
                        for (df, dr, side) in [
                            (0, 2, color.opposite()),
                            (2, 2, color.opposite()),
                            (-2, 2, color),
                            (2, 0, color),
                            (1, -3, color.opposite()),
                            (-4, -2, color.opposite()),
                            (-2, -2, color.opposite()),
                        ] {
                            if let Some(pos) = origin.offset(df, dr) {
                                state.place_piece(Piece::new(
                                    if layout == 2 {
                                        PieceType::King
                                    } else {
                                        PieceType::Pawn
                                    },
                                    side,
                                    pos,
                                ));
                            }
                        }
                    }
                    let first_directions: Vec<_> = if kind == PieceType::HookMover {
                        vec![(0, 1), (1, 0), (0, -1), (-1, 0)]
                    } else if kind == PieceType::Peacock {
                        let forward = if color == Color::Black { 1 } else { -1 };
                        vec![(1, forward), (-1, forward)]
                    } else {
                        vec![(1, 1), (1, -1), (-1, -1), (-1, 1)]
                    };
                    let board = state.get_board();
                    let mut expected = BTreeSet::new();
                    let mut captures = BTreeSet::new();
                    let mut routes = Vec::new();
                    for (dx, dy) in first_directions {
                        for corner in walk(board, piece, origin, (dx, dy), 35) {
                            expected.insert(corner);
                            if board.get_piece(corner).is_some() {
                                captures.insert(corner);
                                continue; // First capture ends the move.
                            }
                            for turn in [(-dy, dx), (dy, -dx)] {
                                for to in walk(board, piece, corner, turn, 35) {
                                    expected.insert(to);
                                    if board.get_piece(to).is_some() {
                                        captures.insert(to);
                                    }
                                    routes.push((corner, to));
                                }
                            }
                        }
                    }
                    if kind == PieceType::Peacock {
                        let back = if color == Color::Black { -1 } else { 1 };
                        for direction in [(1, back), (-1, back)] {
                            for to in walk(board, piece, origin, direction, 2) {
                                expected.insert(to);
                                if board.get_piece(to).is_some() {
                                    captures.insert(to);
                                }
                            }
                        }
                    }
                    let generated = state.generate_legal_moves_for_pieces(&[piece]);
                    assert_eq!(
                        generated.iter().map(|mv| mv.to).collect::<BTreeSet<_>>(),
                        expected,
                        "{piece:?} layout={layout}"
                    );
                    let config = MovementConfig::for_piece(&piece);
                    assert_eq!(
                        MovementGenerator::generate_targets(&piece, board, &config.capabilities)
                            .into_iter()
                            .collect::<BTreeSet<_>>(),
                        expected
                    );
                    for mv in &generated {
                        if let Some(corner) = mv.intermediate() {
                            assert!(
                                routes.contains(&(corner, mv.to)),
                                "invalid generated route {piece:?} {mv:?}"
                            );
                        }
                    }
                    for mode in [
                        LegalMoveGen::CapturesOnly,
                        LegalMoveGen::WithoutQuietMultiLeg,
                        LegalMoveGen::QuietMultiLegOnly,
                    ] {
                        let mut moves = Vec::new();
                        state.generate_legal_moves_for_pieces_mode(&[piece], mode, &mut moves);
                        if mode == LegalMoveGen::CapturesOnly {
                            assert_eq!(
                                moves.iter().map(|mv| mv.to).collect::<BTreeSet<_>>(),
                                captures
                            );
                        }
                        for mv in moves {
                            assert!(
                                generated.contains(&mv),
                                "stage emitted a different/invalid move {mv:?}"
                            );
                        }
                    }
                    for victim in board.pieces_by_color(color.opposite()) {
                        let hits = crate::search::generate_captures_hitting_square(
                            &state,
                            victim.position,
                        );
                        assert_eq!(
                            hits.iter().any(|mv| mv.from == origin),
                            captures.contains(&victim.position),
                            "{piece:?} victim={victim:?}"
                        );
                        for mv in hits.iter().filter(|mv| mv.from == origin) {
                            assert_eq!(
                                mv.to, victim.position,
                                "range movers can only take their final landing"
                            );
                            if let Some(corner) = mv.intermediate() {
                                assert!(routes.contains(&(corner, mv.to)));
                            }
                        }
                    }
                    // Friendly blockers can attack too. Query the single-mover
                    // list path to isolate this piece's attack map.
                    for rank in 0..36 {
                        for file in 0..36 {
                            let target = p(file, rank);
                            assert_eq!(
                                crate::board::is_position_attacked_by_pieces(
                                    board,
                                    target,
                                    &[piece],
                                    false
                                ),
                                expected.contains(&target),
                                "{piece:?} target={target:?} layout={layout}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn range_continuations_reject_straight_reverse_and_capture_first_atomically() {
    let network = crate::nnue::tests::net(32, "range-turn-rejection");
    for kind in [
        PieceType::HookMover,
        PieceType::Tengu,
        PieceType::Capricorn,
        PieceType::Peacock,
    ] {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                let origin = p(18, 18);
                let mut piece = Piece::new(kind, color, origin);
                if promoted {
                    piece.promote();
                }
                let orthogonal = piece.piece_type == PieceType::HookMover;
                let dy = if color == Color::Black { 1 } else { -1 };
                let dx = if orthogonal { 0 } else { 1 };
                let corner = origin.offset(3 * dx, 3 * dy).unwrap();
                let turn_target = corner.offset(2 * dy, -2 * dx).unwrap();
                for capture_first in [false, true] {
                    let mut state = GameState::new();
                    state.set_current_turn(color);
                    state.place_piece(piece);
                    state.place_piece(Piece::new(PieceType::King, Color::Black, p(0, 0)));
                    state.place_piece(Piece::new(PieceType::King, Color::White, p(35, 35)));
                    state.place_piece(Piece::new(
                        PieceType::WoodlandDemon,
                        color.opposite(),
                        turn_target,
                    ));
                    if capture_first {
                        state.place_piece(Piece::new(
                            PieceType::WoodlandDemon,
                            color.opposite(),
                            corner,
                        ));
                    }
                    state.set_turns_without_capture_or_promotion(37);
                    let mut weights = crate::eval::EvalWeights::seed();
                    weights.noise_scale = 0.0;
                    weights.nnue_runtime = Some(network.clone());
                    state.ensure_eval_inc(&weights);
                    let hash = state.hash;
                    let repetition = state.rep_history.clone();
                    let score = crate::eval::evaluate_with_ply(&state, &weights, 0);
                    let mut illegal = vec![
                        origin,
                        origin.offset(-2 * dx, -2 * dy).unwrap(),
                        corner.offset(2 * dx, 2 * dy).unwrap(),
                    ];
                    if capture_first {
                        illegal.push(turn_target);
                    }
                    for target in illegal {
                        let mv = Move::new_two_step(origin, corner, target);
                        assert!(!state
                            .generate_legal_moves_for_pieces(&[piece])
                            .contains(&mv));
                        let mut manual = state.clone();
                        manual.make_move(mv.clone());
                        assert_eq!(manual.hash, hash, "{piece:?} {mv:?}");
                        assert_eq!(manual.get_current_turn(), color);
                        let mut searched = state.clone();
                        assert!(searched
                            .make_move_for_search_with_weights(mv, &weights)
                            .is_none());
                        for unchanged in [&manual, &searched] {
                            assert_eq!(unchanged.hash, hash);
                            assert_eq!(unchanged.rep_history, repetition);
                            assert_eq!(unchanged.get_move_history(), state.get_move_history());
                            assert_eq!(unchanged.get_board().get_piece(origin), Some(piece));
                            assert_eq!(
                                unchanged.get_board().get_piece(corner),
                                state.get_board().get_piece(corner)
                            );
                            assert_eq!(
                                crate::eval::evaluate_with_ply(unchanged, &weights, 0),
                                score
                            );
                            assert!(unchanged
                                .nnue
                                .as_ref()
                                .unwrap()
                                .matches_rebuild(unchanged.get_board()));
                        }
                    }
                    if capture_first {
                        assert!(state
                            .generate_legal_moves_for_pieces(&[piece])
                            .iter()
                            .any(|mv| mv.to == corner && !mv.is_two_step()));
                    } else {
                        let mv = Move::new_two_step(origin, corner, turn_target);
                        let mut moved = state.clone();
                        moved.make_move(mv.clone());
                        assert_eq!(moved.get_current_turn(), color.opposite());
                        assert_eq!(
                            moved.get_board().get_piece(turn_target).unwrap().piece_type,
                            piece.piece_type
                        );
                        let undo = state
                            .make_move_for_search_with_weights(mv, &weights)
                            .unwrap();
                        assert_eq!(state.hash, moved.hash);
                        assert_eq!(state.hash, crate::zobrist::compute(&state));
                        assert!(state
                            .nnue
                            .as_ref()
                            .unwrap()
                            .matches_rebuild(state.get_board()));
                        let mut rebuilt = moved.clone();
                        rebuilt.ensure_eval_inc(&weights);
                        assert_eq!(
                            crate::eval::evaluate_with_ply(&state, &weights, 0),
                            crate::eval::evaluate_with_ply(&rebuilt, &weights, 0)
                        );
                        state.unmake_move_for_search(undo);
                        assert_eq!(state.hash, hash);
                        assert_eq!(state.rep_history, repetition);
                        assert!(state
                            .nnue
                            .as_ref()
                            .unwrap()
                            .matches_rebuild(state.get_board()));
                        assert_eq!(crate::eval::evaluate_with_ply(&state, &weights, 0), score);
                    }
                }
            }
        }
    }
}

#[test]
fn buddhist_spirit_has_queen_slides_and_two_square_jumps_without_a_second_step() {
    for color in [Color::Black, Color::White] {
        let origin = p(18, 18);
        let piece = Piece::new(PieceType::BuddhistSpirit, color, origin);
        let mut state = GameState::new();
        state.set_current_turn(color);
        state.place_piece(piece);
        let expected: BTreeSet<_> = (0..36)
            .flat_map(|rank| (0..36).map(move |file| p(file, rank)))
            .filter(|to| {
                let dx = (i16::from(to.file()) - i16::from(origin.file())).abs();
                let dy = (i16::from(to.rank()) - i16::from(origin.rank())).abs();
                *to != origin && (dx == 0 || dy == 0 || dx == dy || dx.max(dy) == 2)
            })
            .collect();
        let moves = state.generate_legal_moves_for_pieces(&[piece]);
        assert!(moves.iter().all(|mv| !mv.is_two_step()));
        assert_eq!(
            moves.iter().map(|mv| mv.to).collect::<BTreeSet<_>>(),
            expected
        );
        assert!(MovementConfig::for_piece(&piece)
            .capabilities
            .iter()
            .all(|cap| !matches!(cap, MovementCapability::TwoStep { .. })));
        let via = p(18, 28);
        let target = p(19, 28);
        state.place_piece(Piece::new(PieceType::King, color.opposite(), target));
        assert!(!state
            .get_board()
            .is_position_attacked_by_color_for_check(target, color));
        assert!(crate::search::generate_captures_hitting_square(&state, target).is_empty());
        let old_extra_step = Move::new_two_step(origin, via, target);
        let hash = state.hash;
        let mut manual = state.clone();
        manual.make_move(old_extra_step.clone());
        assert_eq!(manual.hash, hash);
        assert!(state.make_move_for_search(old_extra_step).is_none());
        // Its separate non-ray jump still works over an occupied first square.
        state.place_piece(Piece::new(PieceType::Pawn, color, p(19, 18)));
        let jump = Move::new(origin, p(20, 19));
        assert!(state
            .generate_legal_moves_for_pieces(&[piece])
            .contains(&jump));
        state.make_move(jump);
        assert_eq!(state.get_current_turn(), color.opposite());
        assert!(state.get_board().get_piece(p(19, 18)).is_some());
    }
}

#[test]
fn short_two_movers_and_free_eagle_still_capture_and_return() {
    for kind in [
        PieceType::Lion,
        PieceType::LionHawk,
        PieceType::FuriousFiend,
        PieceType::FreeEagle,
    ] {
        for color in [Color::Black, Color::White] {
            let mut state = GameState::new();
            state.set_current_turn(color);
            let origin = p(18, 18);
            let via = p(19, 19);
            state.place_piece(Piece::new(kind, color, origin));
            state.place_piece(Piece::new(PieceType::Pawn, color.opposite(), via));
            let mv = if kind == PieceType::FreeEagle {
                Move::new_free_eagle(origin, origin, vec![origin, via, origin])
            } else {
                Move::new_two_step(origin, via, origin)
            };
            assert!(
                state.generate_legal_moves().contains(&mv),
                "{kind:?} {color:?}"
            );
            state.make_move(mv);
            assert_eq!(state.get_current_turn(), color.opposite());
            assert!(state.get_board().get_piece(via).is_none());
            assert_eq!(
                state.get_board().get_piece(origin).unwrap().piece_type,
                kind
            );
        }
    }
}

#[test]
fn entering_promotion_zone_at_an_empty_corner_still_allows_promotion() {
    for color in [Color::Black, Color::White] {
        let square = |file: u8, rank: u8| {
            if color == Color::Black {
                p(file, rank)
            } else {
                p(35 - file, 35 - rank)
            }
        };
        let origin = square(18, 24);
        let via = square(19, 25);
        let to = square(20, 24);
        let mut state = GameState::new();
        state.set_current_turn(color);
        state.place_piece(Piece::new(PieceType::Capricorn, color, origin));
        let mv = Move::new_two_step_with_promotion(origin, via, to, true);
        assert!(state.generate_legal_moves().contains(&mv));
        state.make_move(mv);
        assert_eq!(state.get_current_turn(), color.opposite());
        assert_eq!(
            state.get_board().get_piece(to).unwrap().piece_type,
            PieceType::HookMover
        );
    }
}

#[test]
fn initial_armies_rotate_both_axes_and_left_chariot_keeps_its_diagonal_axis() {
    let mut opening = GameState::new();
    opening.setup_initial_position();
    for black in opening.get_board().pieces_by_color(Color::Black) {
        let white = opening
            .get_board()
            .get_piece(p(35 - black.position.file(), 35 - black.position.rank()))
            .unwrap();
        assert_eq!(white.color, Color::White);
        assert_eq!(white.piece_type, black.piece_type);
    }
    for color in [Color::Black, Color::White] {
        let origin = p(18, 18);
        let piece = Piece::new(PieceType::LeftChariot, color, origin);
        let mut board = Board::new();
        board.place_piece(piece);
        let targets = piece.get_potential_targets(&board);
        assert!(targets.contains(&p(25, 25)));
        assert!(targets.contains(&p(11, 11)));
        assert!(!targets.contains(&p(25, 11)));
        assert!(!targets.contains(&p(11, 25)));
        let left = if color == Color::Black { -1 } else { 1 };
        assert!(targets.contains(&origin.offset(left, 0).unwrap()));
        assert!(!targets.contains(&origin.offset(-left, 0).unwrap()));
    }
}
