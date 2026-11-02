//! Cross-check independently used movement entry points for every piece family.
use super::*;
use crate::movement::{MovementConfig, MovementGenerator};
use crate::piece::PieceType;
use std::collections::BTreeSet;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

#[test]
fn movement_rotates_with_color_for_every_piece_and_promotion() {
    let rotate = |pos: Position| p(35 - pos.file(), 35 - pos.rank());
    let mut failures = Vec::new();
    for &kind in crate::eval::ALL_PIECE_TYPES {
        for promoted in [false, true] {
            let mut black = Piece::new(kind, Color::Black, p(12, 15));
            if promoted {
                black.promote();
            }
            let mut white = black;
            white.color = Color::White;
            white.position = rotate(black.position);
            let mut black_board = Board::new();
            let mut white_board = Board::new();
            black_board.place_piece(black);
            white_board.place_piece(white);
            let expected: BTreeSet<_> = black
                .get_potential_targets(&black_board)
                .into_iter()
                .map(rotate)
                .collect();
            let actual: BTreeSet<_> = white
                .get_potential_targets(&white_board)
                .into_iter()
                .collect();
            if expected != actual {
                failures.push(format!(
                    "{kind:?} promoted={promoted}: missing {:?}; extra {:?}",
                    expected.difference(&actual).take(2).collect::<Vec<_>>(),
                    actual.difference(&expected).take(2).collect::<Vec<_>>()
                ));
            }
            // Empty-board sliders can hide an incorrectly oriented jump when
            // they reach the same landing. Screen every first step to isolate
            // the leaping capabilities as well.
            for (df, dr) in [
                (0, 1),
                (1, 1),
                (1, 0),
                (1, -1),
                (0, -1),
                (-1, -1),
                (-1, 0),
                (-1, 1),
            ] {
                let position = black.position.offset(df, dr).unwrap();
                black_board.place_piece(Piece::new(PieceType::Pawn, Color::Black, position));
                white_board.place_piece(Piece::new(
                    PieceType::Pawn,
                    Color::White,
                    rotate(position),
                ));
            }
            let expected: BTreeSet<_> = black
                .get_potential_targets(&black_board)
                .into_iter()
                .map(rotate)
                .collect();
            let actual: BTreeSet<_> = white
                .get_potential_targets(&white_board)
                .into_iter()
                .collect();
            if expected != actual {
                failures.push(format!(
                    "screened {kind:?} promoted={promoted}: missing {:?}; extra {:?}",
                    expected.difference(&actual).take(2).collect::<Vec<_>>(),
                    actual.difference(&expected).take(2).collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn vice_general_orthogonal_jump_leaves_the_jumped_piece_alive() {
    use crate::move_simulation::{simulate_move, BoardLike};

    for color in [Color::Black, Color::White] {
        for mid_color in [color, color.opposite()] {
            for kind in [PieceType::Pawn, PieceType::King] {
                let mut state = GameState::new();
                state.set_current_turn(color);
                let piece = Piece::new(PieceType::ViceGeneral, color, p(18, 18));
                let midpoint = p(18, 19);
                let target = p(18, 20);
                let jumped = Piece::new(kind, mid_color, midpoint);
                state.place_piece(piece);
                state.place_piece(jumped);
                let mv = Move::new(piece.position, target);
                assert!(state
                    .generate_legal_moves_for_pieces(&[piece])
                    .contains(&mv));
                let simulated = simulate_move(state.get_board(), &mv, &piece);
                assert_eq!(simulated.get_piece(midpoint), Some(jumped), "simulation");
                assert!(!crate::search::move_captures_enemy(&state, &mv));
                assert!(!crate::search::capture_hits_square(&state, &mv, midpoint));
                let mut moved = state.clone();
                moved.make_move(mv.clone());
                assert_eq!(moved.get_current_turn(), color.opposite());
                assert_eq!(
                    moved.get_board().get_piece(midpoint),
                    Some(jumped),
                    "execution"
                );
                let mut searched = state.clone();
                let undo = searched.make_move_for_search(mv).unwrap();
                assert_eq!(
                    searched.get_board().get_piece(midpoint),
                    Some(jumped),
                    "search"
                );
                searched.unmake_move_for_search(undo);
                assert_eq!(searched.get_board().get_piece(midpoint), Some(jumped));
            }
        }
    }
}

#[test]
fn flying_crocodile_has_three_forward_and_two_backward_diagonal_steps() {
    // The piece definition and movement-config contract both specify forward /
    // backward diagonals, rather than the old right / left diagonal masks.
    for color in [Color::Black, Color::White] {
        let origin = p(18, 18);
        let piece = Piece::new(PieceType::FlyingCrocodile, color, origin);
        let mut board = Board::new();
        board.place_piece(piece);
        let mut expected = BTreeSet::new();
        for distance in 1..36 {
            for (df, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                if let Some(pos) = origin.offset(df * distance, dr * distance) {
                    expected.insert(pos);
                }
            }
        }
        let forward = if color == Color::Black { 1 } else { -1 };
        for file_sign in [-1, 1] {
            for distance in 1..=3 {
                expected.insert(
                    origin
                        .offset(file_sign * distance, forward * distance)
                        .unwrap(),
                );
            }
            for distance in 1..=2 {
                expected.insert(
                    origin
                        .offset(file_sign * distance, -forward * distance)
                        .unwrap(),
                );
            }
        }
        let actual: BTreeSet<_> = piece.get_potential_targets(&board).into_iter().collect();
        assert_eq!(actual, expected, "{color:?}");
    }
}

#[test]
fn generated_destinations_are_detected_as_attacked_for_every_piece() {
    let mut failures = Vec::new();
    for &kind in crate::eval::ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                let mut piece = Piece::new(kind, color, p(18, 18));
                if promoted {
                    piece.promote();
                }
                let mut board = Board::new();
                board.place_piece(piece);
                let targets = piece.get_potential_targets(&board);
                let missing: Vec<_> = targets
                    .into_iter()
                    .filter(|target| *target != piece.position)
                    .filter(|target| !board.is_position_attacked_by_color(*target, color))
                    .collect();
                if !missing.is_empty() {
                    failures.push(format!(
                        "{kind:?} {color:?} promoted={promoted}: {} missing, e.g. {:?}",
                        missing.len(),
                        &missing[..missing.len().min(4)]
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn long_multileg_attacks_also_reach_victim_directed_capture_generation() {
    for color in [Color::Black, Color::White] {
        for (kind, df, dr) in [(PieceType::LionHawk, 10, 8), (PieceType::Peacock, 8, 0)] {
            let mut state = GameState::new();
            state.set_current_turn(color);
            let origin = p(18, 18);
            let piece = Piece::new(kind, color, origin);
            let sign = if color == Color::Black { 1 } else { -1 };
            let victim = origin.offset(sign * df, sign * dr).unwrap();
            state.place_piece(piece);
            state.place_piece(Piece::new(PieceType::King, color.opposite(), victim));
            assert!(state
                .generate_legal_moves_for_pieces(&[piece])
                .iter()
                .any(|mv| mv.to == victim));
            assert!(state
                .get_board()
                .is_position_attacked_by_color_for_check(victim, color));
            let directed = crate::search::generate_captures_hitting_square(&state, victim);
            assert!(
                directed.iter().any(|mv| mv.to == victim),
                "{piece:?} {victim:?}"
            );
            for mv in directed {
                let mut child = state.clone();
                child.make_move(mv.clone());
                assert_eq!(child.get_current_turn(), color.opposite(), "{mv:?}");
                assert_eq!(child.get_board().royal_count(color.opposite()), 0, "{mv:?}");
            }
        }
    }
}

fn check_attack_maps_with_blockers_and_at_edges(group: usize) {
    use crate::move_simulation::{BoardLike, MoveDelta, VirtualBoard};

    // Disjoint groups retain the complete matrix and let libtest schedule this
    // long audit alongside the other tests instead of leaving a serial tail.
    for &kind in crate::eval::ALL_PIECE_TYPES.iter().skip(group).step_by(4) {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                for origin in [p(18, 18), p(1, 34)] {
                    let mut piece = Piece::new(kind, color, origin);
                    if promoted {
                        piece.promote();
                    }
                    let mut board = Board::new();
                    board.place_piece(piece);
                    for (df, dr, occupant_kind, side) in [
                        (1, 0, PieceType::Pawn, color),
                        (-1, 1, PieceType::Pawn, color),
                        (0, 2, PieceType::Pawn, color.opposite()),
                        (3, -3, PieceType::Pawn, color.opposite()),
                        (-3, -3, PieceType::Pawn, color),
                        (7, 9, PieceType::King, color.opposite()),
                        (9, 7, PieceType::CrownPrince, color.opposite()),
                        (0, -7, PieceType::King, color.opposite()),
                    ] {
                        if let Some(pos) = origin.offset(df, dr) {
                            board.place_piece(Piece::new(occupant_kind, side, pos));
                        }
                    }
                    let expected: BTreeSet<_> = board
                        .pieces_by_color(color)
                        .iter()
                        .flat_map(|attacker| {
                            attacker
                                .get_potential_targets(&board)
                                .into_iter()
                                .filter(|target| *target != attacker.position)
                        })
                        .collect();
                    let virtual_board = VirtualBoard::new(&board, MoveDelta::new());
                    for rank in 0..36 {
                        for file in 0..36 {
                            let target = p(file, rank);
                            let hit = expected.contains(&target);
                            assert_eq!(
                                board.is_position_attacked_by_color(target, color),
                                hit,
                                "{piece:?} target={target:?}"
                            );
                            // Exercise the list-based path too, outside the board's
                            // square-mask candidate index, including every royal.
                            if (file + rank) % 13 == 0
                                || board
                                    .get_piece(target)
                                    .is_some_and(|p| p.piece_type.is_royal())
                            {
                                assert_eq!(
                                    virtual_board.is_position_attacked_by_color(target, color),
                                    hit,
                                    "virtual {piece:?} target={target:?}"
                                );
                                if board
                                    .get_piece(target)
                                    .is_some_and(|p| p.piece_type.is_royal())
                                {
                                    assert_eq!(
                                        board
                                            .is_position_attacked_by_color_for_check(target, color),
                                        hit,
                                        "royal {piece:?} target={target:?}"
                                    );
                                    assert_eq!(
                                        virtual_board
                                            .is_position_attacked_by_color_for_check(target, color),
                                        hit,
                                        "virtual royal {piece:?} target={target:?}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

macro_rules! attack_map_groups {
    ($($name:ident: $group:expr),+ $(,)?) => {
        $(#[test]
        fn $name() {
            check_attack_maps_with_blockers_and_at_edges($group);
        })+
    };
}

attack_map_groups! {
    attack_maps_match_generated_reach_with_blockers_and_at_edges_group_0: 0,
    attack_maps_match_generated_reach_with_blockers_and_at_edges_group_1: 1,
    attack_maps_match_generated_reach_with_blockers_and_at_edges_group_2: 2,
    attack_maps_match_generated_reach_with_blockers_and_at_edges_group_3: 3,
}

#[test]
fn generated_moves_apply_simulate_and_unmake_consistently_for_every_piece() {
    use crate::move_simulation::{simulate_move, BoardLike};

    let board_pieces = |board: &dyn BoardLike| {
        let mut pieces = board.get_pieces_by_color(Color::Black);
        pieces.extend(board.get_pieces_by_color(Color::White));
        pieces.sort_by_key(|piece| piece.position);
        pieces
    };
    for &kind in crate::eval::ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            for promoted in [false, true] {
                for origin in [p(18, 18), p(18, 25), p(1, 1)] {
                    let mut piece = Piece::new(kind, color, origin);
                    if promoted {
                        piece.promote();
                    }
                    let mut state = GameState::new();
                    state.set_current_turn(color);
                    state.place_piece(piece);
                    state.set_turns_without_capture_or_promotion(31);
                    for (df, dr, side) in [
                        (1, 0, color),
                        (0, 1, color.opposite()),
                        (2, 2, color.opposite()),
                        (3, -3, color.opposite()),
                        (-2, 2, color),
                        (-1, -1, color.opposite()),
                    ] {
                        if let Some(pos) = origin.offset(df, dr) {
                            state.place_piece(Piece::new(PieceType::Pawn, side, pos));
                        }
                    }
                    let before = board_pieces(state.get_board());
                    let hash = state.hash;
                    let moves = state.generate_legal_moves_for_pieces(&[piece]);
                    // Spread the sample across the whole list: the first few
                    // destinations alone miss promotions and multi-leg routes.
                    let stride = moves.len().div_ceil(24).max(1);
                    for mv in moves.iter().step_by(stride).chain(moves.last()) {
                        let simulated = simulate_move(state.get_board(), mv, &piece);
                        let expected = board_pieces(&simulated);
                        let mut manual = state.clone();
                        manual.make_move(mv.clone());
                        assert_eq!(
                            manual.get_current_turn(),
                            color.opposite(),
                            "{piece:?} {mv:?}"
                        );
                        assert_eq!(
                            board_pieces(manual.get_board()),
                            expected,
                            "{piece:?} {mv:?}"
                        );
                        let mut searched = state.clone();
                        let undo = searched.make_move_for_search(mv.clone()).unwrap();
                        assert_eq!(
                            board_pieces(searched.get_board()),
                            expected,
                            "{piece:?} {mv:?}"
                        );
                        assert_eq!(
                            searched.hash,
                            crate::zobrist::compute(&searched),
                            "{piece:?} {mv:?}"
                        );
                        assert_eq!(searched.hash, manual.hash, "{piece:?} {mv:?}");
                        assert_eq!(
                            searched.get_turns_without_capture_or_promotion(),
                            manual.get_turns_without_capture_or_promotion()
                        );
                        searched.unmake_move_for_search(undo);
                        assert_eq!(
                            board_pieces(searched.get_board()),
                            before,
                            "{piece:?} {mv:?}"
                        );
                        assert_eq!(searched.hash, hash, "{piece:?} {mv:?}");
                        assert_eq!(searched.rep_history, state.rep_history);
                    }
                }
            }
        }
    }
}

#[test]
fn capture_only_generation_matches_captures_from_full_generation_for_every_piece() {
    let mut failures = Vec::new();
    for &kind in crate::eval::ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            let piece = Piece::new(kind, color, p(18, 18));
            let mut state = GameState::new();
            state.set_current_turn(color);
            state.place_piece(piece);
            for (df, dr, side) in [
                (1, 0, color),
                (0, 2, color.opposite()),
                (1, 1, color.opposite()),
                (3, 3, color.opposite()),
                (-2, -2, color.opposite()),
                (-4, 0, color.opposite()),
                (0, -3, color),
                (-2, 2, color),
            ] {
                state.place_piece(Piece::new(
                    PieceType::Pawn,
                    side,
                    piece.position.offset(df, dr).unwrap(),
                ));
            }
            let full = state.generate_legal_moves_for_pieces(&[piece]);
            let mut captures = Vec::new();
            state.generate_legal_moves_for_pieces_mode(
                &[piece],
                LegalMoveGen::CapturesOnly,
                &mut captures,
            );
            let expected: BTreeSet<_> = full
                .iter()
                .filter(|mv| crate::search::move_captures_enemy(&state, mv))
                .map(|mv| format!("{mv:?}"))
                .collect();
            let actual: BTreeSet<_> = captures.iter().map(|mv| format!("{mv:?}")).collect();
            if expected != actual {
                failures.push(format!(
                    "{kind:?} {color:?}: missing {:?}; extra {:?}",
                    expected.difference(&actual).take(3).collect::<Vec<_>>(),
                    actual.difference(&expected).take(3).collect::<Vec<_>>()
                ));
            }
            // The generic target API is also used outside GameState.
            let config = MovementConfig::for_piece(&piece);
            let generic = MovementGenerator::generate_targets_filtered(
                &piece,
                state.get_board(),
                &config.capabilities,
                true,
            );
            let actual: BTreeSet<_> = generic.into_iter().collect();
            let expected: BTreeSet<_> = full
                .iter()
                .filter(|mv| crate::search::move_captures_enemy(&state, mv))
                .map(|mv| mv.to)
                .collect();
            if expected != actual {
                failures.push(format!(
                    "generic {kind:?} {color:?}: missing {:?}; extra {:?}",
                    expected.difference(&actual).take(3).collect::<Vec<_>>(),
                    actual.difference(&expected).take(3).collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
