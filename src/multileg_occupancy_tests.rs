//! Cross-check multi-leg occupancy against physically executing the first leg.
use crate::board_position::BoardPosition;
use crate::game_state::{GameState, LegalMoveGen, Move};
use crate::move_simulation::{simulate_move, BoardAfterStep, BoardLike};
use crate::movement::{MovementCapability, MovementConfig, MovementGenerator};
use crate::piece::{Color, Piece, PieceType};
use crate::position::Position;
use std::collections::BTreeSet;

fn p(file: u8, rank: u8) -> Position {
    Position::new(file, rank).unwrap()
}

fn snapshot(state: &GameState) -> BoardPosition {
    let mut result = BoardPosition::from_state(state);
    result.pieces.sort_by_key(|piece| piece.position);
    result
}

#[test]
fn slot5884_capricorn_can_capture_and_retreat_to_or_past_its_origin() {
    // Reduced position before recorded ply121. Coordinates are internal (0..35).
    let from = p(17, 27);
    let victim = p(17, 33);
    let mut capricorn = Piece::new(PieceType::Capricorn, Color::Black, from);
    capricorn.promote();
    let mut state = GameState::new();
    for piece in [
        capricorn,
        Piece::new(PieceType::WoodlandDemon, Color::White, victim),
        Piece::new(PieceType::CeramicDove, Color::White, p(14, 25)),
        Piece::new(PieceType::Lion, Color::White, p(16, 30)),
        Piece::new(PieceType::King, Color::Black, p(17, 0)),
        Piece::new(PieceType::CrownPrince, Color::White, p(17, 35)),
    ] {
        state.place_piece(piece);
    }
    state.set_turns_without_capture_or_promotion(37);
    let before = snapshot(&state);
    let hash = state.hash;
    let history = state.rep_history.clone();
    for target in [from, p(17, 26), p(17, 20)] {
        let mv = Move::new_two_step(from, victim, target);
        for mode in [
            LegalMoveGen::All,
            LegalMoveGen::CapturesOnly,
            LegalMoveGen::WithoutQuietMultiLeg,
        ] {
            assert!(
                state.generate_legal_moves_mode(mode).contains(&mv),
                "missing {mv:?} in {mode:?}"
            );
        }
        assert!(!state
            .generate_legal_moves_mode(LegalMoveGen::QuietMultiLegOnly)
            .contains(&mv));
        let mut manual = state.clone();
        manual.make_move(mv.clone());
        assert_eq!(manual.get_current_turn(), Color::White);
        assert!(manual.get_board().get_piece(victim).is_none());
        assert_eq!(
            manual.get_board().get_piece(target).unwrap().base_type(),
            PieceType::Capricorn
        );
        assert_eq!(manual.get_turns_without_capture_or_promotion(), 0);
        let mut searched = state.clone();
        let undo = searched.make_move_for_search(mv).unwrap();
        assert_eq!(snapshot(&searched), snapshot(&manual));
        assert_eq!(searched.hash, crate::zobrist::compute(&searched));
        searched.unmake_move_for_search(undo);
        assert_eq!(snapshot(&searched), before);
        assert_eq!(searched.hash, hash);
        assert_eq!(searched.rep_history, history);
    }
}

#[test]
fn all_two_step_families_match_a_physically_updated_first_leg() {
    let mut checked = 0;
    for &kind in crate::eval::ALL_PIECE_TYPES {
        for color in [Color::Black, Color::White] {
            for origin in [p(18, 18), p(1, 1), p(34, 34)] {
                let piece = Piece::new(kind, color, origin);
                let config = MovementConfig::for_piece(&piece);
                if !config
                    .capabilities
                    .iter()
                    .any(|c| matches!(c, MovementCapability::TwoStep { .. }))
                {
                    continue;
                }
                checked += 1;
                let mut state = GameState::new();
                state.set_current_turn(color);
                state.place_piece(piece);
                for (df, dr, occupant_color) in [
                    (1, 0, color.opposite()),
                    (0, 1, color.opposite()),
                    (1, 1, color.opposite()),
                    (-1, 1, color.opposite()),
                    (-1, -1, color.opposite()),
                    (0, -1, color.opposite()),
                    (-3, 0, color),
                    (0, 3, color),
                ] {
                    if let Some(pos) = origin.offset(df, dr) {
                        state.place_piece(Piece::new(PieceType::Pawn, occupant_color, pos));
                    }
                }
                let mut expected_targets = BTreeSet::new();
                let mut expected_capture_targets = BTreeSet::new();
                let mut expected_capture_routes = BTreeSet::new();
                for capability in &config.capabilities {
                    let MovementCapability::TwoStep { first, second } = capability else {
                        for target in MovementGenerator::capability_landings(
                            &piece,
                            state.get_board(),
                            capability,
                        ) {
                            expected_targets.insert(target);
                            if state
                                .get_board()
                                .get_piece(target)
                                .is_some_and(|p| p.color != color)
                            {
                                expected_capture_targets.insert(target);
                            }
                        }
                        continue;
                    };
                    for mid in
                        MovementGenerator::capability_landings(&piece, state.get_board(), first)
                    {
                        expected_targets.insert(mid);
                        let first_capture = state
                            .get_board()
                            .get_piece(mid)
                            .is_some_and(|p| p.color != color);
                        if first_capture {
                            expected_capture_targets.insert(mid);
                        }
                        // Independent reference: mutate a real board, rather than use the overlay.
                        let mut physical = state.clone_board();
                        physical.move_piece(origin, mid);
                        let moved = physical.get_piece(mid).unwrap();
                        let after = BoardAfterStep::new(state.get_board(), piece, mid);
                        for position in [origin, mid, p(0, 0), p(35, 35)] {
                            assert_eq!(after.get_piece(position), physical.get_piece(position));
                        }
                        for side in [color, color.opposite()] {
                            let mut actual = after.get_pieces_by_color(side);
                            let mut expected = physical.get_pieces_by_color(side);
                            actual.sort_by_key(|p| p.position);
                            expected.sort_by_key(|p| p.position);
                            assert_eq!(actual, expected);
                        }
                        for target in
                            MovementGenerator::capability_landings(&moved, &physical, second)
                        {
                            expected_targets.insert(target);
                            let capture = first_capture
                                || physical.get_piece(target).is_some_and(|p| p.color != color);
                            if capture {
                                expected_capture_targets.insert(target);
                                if first_capture {
                                    expected_capture_routes.insert((mid, target));
                                }
                            }
                        }
                    }
                }
                let actual: BTreeSet<_> = piece
                    .get_potential_targets(state.get_board())
                    .into_iter()
                    .collect();
                assert_eq!(
                    actual, expected_targets,
                    "all targets {kind:?} {color:?} {origin:?}"
                );
                let actual: BTreeSet<_> = MovementGenerator::generate_targets_filtered(
                    &piece,
                    state.get_board(),
                    &config.capabilities,
                    true,
                )
                .into_iter()
                .collect();
                assert_eq!(
                    actual, expected_capture_targets,
                    "capture targets {kind:?} {color:?} {origin:?}"
                );
                let captures = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
                let capture_targets: BTreeSet<_> = captures.iter().map(|mv| mv.to).collect();
                assert_eq!(capture_targets, expected_capture_targets);
                // Empty-intermediate routes may deliberately collapse into a
                // standard capture with the same resulting state. Capturing
                // intermediates must retain every distinct victim route.
                let actual: BTreeSet<_> = captures
                    .iter()
                    .filter_map(|mv| {
                        mv.intermediate()
                            .filter(|mid| {
                                state
                                    .get_board()
                                    .get_piece(*mid)
                                    .is_some_and(|p| p.color != color)
                            })
                            .map(|mid| (mid, mv.to))
                    })
                    .collect();
                assert_eq!(
                    actual, expected_capture_routes,
                    "capture routes {kind:?} {color:?} {origin:?}"
                );
                for victim in state.get_board().get_pieces_by_color(color.opposite()) {
                    assert_eq!(
                        state
                            .get_board()
                            .is_position_attacked_by_color(victim.position, color),
                        expected_targets.contains(&victim.position),
                        "attack detection {kind:?} {color:?} {:?}",
                        victim.position
                    );
                }
                for mv in captures {
                    let mut child = state.clone();
                    child.make_move(mv.clone());
                    assert_eq!(
                        child.get_current_turn(),
                        color.opposite(),
                        "generated move rejected: {kind:?} {mv:?}"
                    );
                }
            }
        }
    }
    assert!(
        checked >= 48,
        "expected all eight two-step families in both colors at three origins, got {checked}"
    );
}

#[test]
fn return_capture_does_not_erase_real_friendly_blockers_or_accept_wrong_turn() {
    let from = p(18, 18);
    let mid = p(18, 22);
    let mut state = GameState::new();
    state.place_piece(Piece::new(PieceType::HookMover, Color::Black, from));
    state.place_piece(Piece::new(PieceType::Pawn, Color::White, mid));
    state.place_piece(Piece::new(PieceType::Pawn, Color::Black, p(18, 16)));
    let blocked = Move::new_two_step(from, mid, p(18, 15));
    assert!(!state.generate_legal_moves().contains(&blocked));
    let before = snapshot(&state);
    state.make_move(blocked);
    assert_eq!(
        snapshot(&state),
        before,
        "failed second leg must leave first-leg victim intact"
    );
    state.set_current_turn(Color::White);
    let before = snapshot(&state);
    state.make_move(Move::new_two_step(from, mid, from));
    assert_eq!(snapshot(&state), before);
}

#[test]
fn free_eagle_returns_keep_every_victim_route_and_simulate_all_captures() {
    for color in [Color::Black, Color::White] {
        let from = p(18, 18);
        let mover = Piece::new(PieceType::FreeEagle, color, from);
        let mut state = GameState::new();
        state.set_current_turn(color);
        state.place_piece(mover);
        let forward = if color == Color::Black { 1 } else { -1 };
        let adjacent = [p(17, 18), p(19, 18)];
        let diagonal = [
            from.offset(-2, forward * 2).unwrap(),
            from.offset(2, forward * 2).unwrap(),
        ];
        for pos in adjacent.into_iter().chain(diagonal) {
            state.place_piece(Piece::new(PieceType::Pawn, color.opposite(), pos));
        }
        let captures = state.generate_legal_moves_mode(LegalMoveGen::CapturesOnly);
        let returns: Vec<_> = captures
            .iter()
            .filter(|mv| mv.from == from && mv.to == from)
            .collect();
        assert_eq!(
            returns.len(),
            4,
            "every adjacent and forward-diagonal return must be available"
        );
        for mv in returns {
            let simulation = simulate_move(state.get_board(), mv, &mover);
            let mut actual = state.clone();
            actual.make_move(mv.clone());
            assert_eq!(actual.get_current_turn(), color.opposite());
            for pos in [from, adjacent[0], adjacent[1], diagonal[0], diagonal[1]] {
                assert_eq!(simulation.get_piece(pos), actual.get_board().get_piece(pos));
            }
            for side in [color, color.opposite()] {
                let mut sim = simulation.get_pieces_by_color(side);
                let mut real = actual.get_board().get_pieces_by_color(side);
                sim.sort_by_key(|p| p.position);
                real.sort_by_key(|p| p.position);
                assert_eq!(sim, real);
            }
        }
    }
}

#[test]
fn virtual_promotion_preserves_base_type_like_a_real_move() {
    let mut state = GameState::new();
    let from = p(18, 24);
    let mover = Piece::new(PieceType::Capricorn, Color::Black, from);
    state.place_piece(mover);
    let mv = Move::new_with_promotion(from, p(19, 25), true);
    let simulation = simulate_move(state.get_board(), &mv, &mover);
    let mut actual = state.clone();
    actual.make_move(mv.clone());
    assert_eq!(actual.get_current_turn(), Color::White);
    assert_eq!(
        simulation.get_piece(mv.to),
        actual.get_board().get_piece(mv.to)
    );
    assert_eq!(
        simulation.get_pieces_by_color(Color::Black),
        actual.get_board().get_pieces_by_color(Color::Black)
    );
}

#[test]
fn invalid_free_eagle_route_is_rejected_before_any_capture() {
    let from = p(18, 18);
    let enemy = p(19, 18);
    let blocked = p(20, 18);
    let mut state = GameState::new();
    state.place_piece(Piece::new(PieceType::FreeEagle, Color::Black, from));
    state.place_piece(Piece::new(PieceType::Pawn, Color::White, enemy));
    state.place_piece(Piece::new(PieceType::Pawn, Color::Black, blocked));
    let before = snapshot(&state);
    let hash = state.hash;
    state.make_move(Move::new_free_eagle(
        from,
        blocked,
        vec![from, enemy, blocked],
    ));
    assert_eq!(snapshot(&state), before);
    assert_eq!(state.hash, hash);
}

#[test]
fn capture_and_return_preserves_nnue_and_handcrafted_incremental_make_unmake() {
    let network = crate::nnue::tests::net(32, "multileg-return");
    for kind in [PieceType::HookMover, PieceType::FreeEagle] {
        for neural in [false, true] {
            let from = p(18, 18);
            let victim = if kind == PieceType::HookMover {
                p(18, 22)
            } else {
                p(19, 18)
            };
            let mut state = GameState::new();
            for piece in [
                Piece::new(kind, Color::Black, from),
                Piece::new(PieceType::WoodlandDemon, Color::White, victim),
                Piece::new(PieceType::King, Color::Black, p(0, 0)),
                Piece::new(PieceType::King, Color::White, p(35, 35)),
            ] {
                state.place_piece(piece);
            }
            let mut weights = crate::eval::EvalWeights::seed();
            weights.noise_scale = 0.0;
            if neural {
                weights.nnue_runtime = Some(network.clone());
            }
            state.ensure_eval_inc(&weights);
            let before = snapshot(&state);
            let score = crate::eval::evaluate_with_ply(&state, &weights, 0);
            let mv = if kind == PieceType::HookMover {
                Move::new_two_step(from, victim, from)
            } else {
                Move::new_free_eagle(from, from, vec![from, victim, from])
            };
            let undo = state
                .make_move_for_search_with_weights(mv, &weights)
                .unwrap();
            if let Some(accumulator) = &state.nnue {
                assert!(accumulator.matches_rebuild(state.get_board()));
            }
            let mut rebuilt = snapshot(&state).to_state().unwrap();
            rebuilt.ensure_eval_inc(&weights);
            assert_eq!(
                crate::eval::evaluate_with_ply(&state, &weights, 0),
                crate::eval::evaluate_with_ply(&rebuilt, &weights, 0)
            );
            state.unmake_move_for_search(undo);
            assert_eq!(snapshot(&state), before);
            assert_eq!(crate::eval::evaluate_with_ply(&state, &weights, 0), score);
            if let Some(accumulator) = &state.nnue {
                assert!(accumulator.matches_rebuild(state.get_board()));
            }
        }
    }
}

#[test]
fn standard_moves_cannot_skip_a_turn_or_an_intermediate_capture() {
    let from = p(18, 18);
    let mut state = GameState::new();
    state.place_piece(Piece::new(PieceType::HookMover, Color::Black, from));
    state.place_piece(Piece::new(PieceType::Pawn, Color::White, p(18, 20)));
    for destination in [p(21, 21), p(18, 22)] {
        let before = snapshot(&state);
        state.make_move(Move::new(from, destination));
        assert_eq!(
            snapshot(&state),
            before,
            "Standard must not teleport through a two-leg route"
        );
    }
    let route = Move::new_two_step(from, p(18, 20), p(18, 22));
    assert!(state.generate_legal_moves().contains(&route));
    state.make_move(route);
    assert_eq!(state.get_current_turn(), Color::White);
    assert!(state.get_board().get_piece(p(18, 20)).is_none());
}
