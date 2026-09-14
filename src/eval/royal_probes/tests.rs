use super::*;
use crate::board_position::BoardPosition;
use crate::piece::PieceType::*;

fn put(s: &mut GameState, pt: PieceType, c: Color, x: u8, y: u8) {
    s.place_piece(Piece::new(pt, c, Position::new(x, y).unwrap()));
}
fn kings() -> GameState {
    let mut s = GameState::new();
    s.clear_board();
    put(&mut s, King, Color::Black, 0, 0);
    put(&mut s, King, Color::White, 35, 35);
    s
}
fn w() -> EvalWeights {
    let mut w = EvalWeights::seed();
    w.noise_scale = 0.;
    w.lr_flight_k = 4000.;
    w.two_mover_align_k = 80.;
    w.two_mover_align_cap = 400.;
    w
}

#[test]
fn alignment_weights_blockers_direction_and_cap() {
    let mut s = kings();
    s.get_board_mut()
        .remove_piece(Position::new(35, 35).unwrap());
    put(&mut s, King, Color::White, 18, 30);
    put(&mut s, HookMover, Color::Black, 18, 18);
    for (k, expected) in [(0., 0.), (40., 40.), (80., 80.), (160., 160.)] {
        let mut weights = w();
        weights.two_mover_align_k = k;
        weights.two_mover_align_cap = 5. * k;
        assert_eq!(term_scores(s.get_board(), &weights).0, expected);
    }
    let _m = set_modes(true, false);
    assert_eq!(term_scores(s.get_board(), &w()).0, 80.);
    for c in [Color::White, Color::Black] {
        put(&mut s, Pawn, c, 18, 24);
        assert_eq!(term_scores(s.get_board(), &w()).0, 0.);
    }
    s.get_board_mut()
        .remove_piece(Position::new(18, 24).unwrap());
    put(&mut s, Tengu, Color::Black, 18, 18);
    assert_eq!(term_scores(s.get_board(), &w()).0, 0.);
    put(&mut s, Tengu, Color::Black, 6, 18);
    assert_eq!(term_scores(s.get_board(), &w()).0, 80.);
    let mut weights = w();
    weights.two_mover_align_cap = 20.;
    assert_eq!(term_scores(s.get_board(), &weights).0, 20.);
}

#[test]
fn verified_flights_include_safe_captures_for_both_royal_types() {
    for royal in [King, CrownPrince] {
        let mut s = kings();
        put(&mut s, royal, Color::Black, 0, 0);
        put(&mut s, Rook, Color::White, 0, 1);
        put(&mut s, Pawn, Color::Black, 1, 0);
        put(&mut s, Pawn, Color::Black, 1, 1);
        assert_eq!(
            last_royal_flight_penalty(s.get_board(), Color::Black, &w()),
            4000.
        );
        assert_eq!(verified_flights(s.get_board(), Color::Black), Some(1));
        assert_eq!(
            verified_flight_penalty(s.get_board(), Color::Black, 4000.),
            2000.
        );
    }
}

#[test]
fn moving_royal_exposes_a_previously_blocked_ray() {
    let mut s = kings();
    s.get_board_mut().remove_piece(Position::new(0, 0).unwrap());
    put(&mut s, King, Color::Black, 0, 1);
    put(&mut s, Rook, Color::White, 0, 0);
    put(&mut s, Rook, Color::White, 1, 0);
    put(&mut s, Pawn, Color::Black, 1, 1);
    put(&mut s, Pawn, Color::Black, 1, 2);
    assert!(!s
        .get_board()
        .is_position_attacked_by_color_for_check(Position::new(0, 2).unwrap(), Color::White));
    assert_eq!(verified_flights(s.get_board(), Color::Black), Some(0));
    assert_eq!(
        last_royal_flight_penalty(s.get_board(), Color::Black, &w()),
        2000.
    );
    assert_eq!(
        verified_flight_penalty(s.get_board(), Color::Black, 4000.),
        4000.
    );
}

#[test]
fn unthreatened_box_and_spare_royal_do_not_trigger_verified_l() {
    let mut s = kings();
    for (x, y) in [(0, 1), (1, 0), (1, 1)] {
        put(&mut s, Pawn, Color::Black, x, y);
    }
    assert_eq!(
        last_royal_flight_penalty(s.get_board(), Color::Black, &w()),
        400.
    );
    assert_eq!(
        verified_flight_penalty(s.get_board(), Color::Black, 4000.),
        0.
    );
    put(&mut s, Rook, Color::White, 0, 1);
    put(&mut s, CrownPrince, Color::Black, 20, 20);
    assert_eq!(verified_flights(s.get_board(), Color::Black), None);
    assert_eq!(
        verified_flight_penalty(s.get_board(), Color::Black, 4000.),
        0.
    );
}

#[test]
fn no_flight_is_not_no_defense() {
    let mut s = kings();
    put(&mut s, CrownPrince, Color::Black, 0, 0);
    put(&mut s, Rook, Color::White, 0, 8);
    put(&mut s, Rook, Color::White, 1, 8);
    put(&mut s, Rook, Color::Black, 4, 1);
    assert_eq!(verified_flights(s.get_board(), Color::Black), Some(0));
    let before = BoardPosition::from_state(&s);
    let evasions = diagnostic_evasions(&mut s).unwrap();
    assert!(evasions
        .iter()
        .any(|m| m.from == Position::new(4, 1).unwrap() && m.to == Position::new(0, 1).unwrap()));
    assert_eq!(evasions.len(), 1);
    assert_eq!(defense_penalty(&evasions), 1000);
    assert_eq!(BoardPosition::from_state(&s), before);
    assert_eq!(defense_penalty(&[]), 0);
}

fn mate_fixture() -> GameState {
    let mut s = GameState::new();
    s.clear_board();
    put(&mut s, King, Color::Black, 35, 35);
    put(&mut s, King, Color::White, 0, 0);
    put(&mut s, Rook, Color::Black, 2, 3);
    put(&mut s, Rook, Color::Black, 1, 8);
    put(&mut s, Pawn, Color::White, 2, 0);
    put(&mut s, Pawn, Color::White, 2, 2);
    s
}

#[test]
fn mating_probe_proves_check_and_restores_state_on_all_paths() {
    let mut s = mate_fixture();
    let before = BoardPosition::from_state(&s);
    for (budget, cap, expected) in [
        (Duration::ZERO, 512, ProbeStatus::Unknown),
        (Duration::from_secs(2), 0, ProbeStatus::Unknown),
        (Duration::from_secs(2), 512, ProbeStatus::Found),
    ] {
        let r = mate_probe(&mut s, budget, cap);
        assert_eq!(r.status, expected, "{r:?}");
        assert_eq!(BoardPosition::from_state(&s), before);
        if let Some(mv) = r.winning_check {
            let mut check = s.clone();
            check.make_move_for_search(mv).unwrap();
            assert!(diagnostic_evasions(&check).unwrap().is_empty());
        }
    }
    let mut safe = kings();
    assert_eq!(
        mate_probe(&mut safe, Duration::from_secs(1), 512).status,
        ProbeStatus::NoMate
    );
    put(&mut safe, CrownPrince, Color::White, 34, 34);
    assert_eq!(
        mate_probe(&mut safe, Duration::from_secs(1), 512).status,
        ProbeStatus::NotApplicable
    );
}

#[test]
fn probes_are_color_symmetric_and_modes_restore() {
    let mut s = kings();
    put(&mut s, Rook, Color::White, 0, 1);
    put(&mut s, Pawn, Color::Black, 1, 0);
    put(&mut s, Pawn, Color::Black, 1, 1);
    let mut flip = BoardPosition::from_state(&s);
    for p in &mut flip.pieces {
        p.position = Position::new(35 - p.position.file, 35 - p.position.rank).unwrap();
        p.color = p.color.opposite();
    }
    flip.turn = flip.turn.opposite();
    let other = flip.to_state();
    {
        let _m = set_modes(true, true);
        assert_eq!(
            term_scores(s.get_board(), &w()).1,
            -term_scores(other.get_board(), &w()).1
        );
    }
    assert!(!blocked_alignment_enabled() && !verified_flights_enabled());
}

#[test]
fn king_two_square_escape_is_not_a_mating_net() {
    let mut s = kings();
    put(&mut s, Rook, Color::White, 0, 8);
    put(&mut s, Rook, Color::White, 1, 8);
    assert_eq!(
        last_royal_flight_penalty(s.get_board(), Color::Black, &w()),
        4000.
    );
    assert_eq!(verified_flights(s.get_board(), Color::Black), Some(2));
    assert_eq!(
        verified_flight_penalty(s.get_board(), Color::Black, 4000.),
        800.
    );
}

#[test]
fn incremental_evaluation_agrees_and_history_survives_probe() {
    let mut s = mate_fixture();
    s.make_move(Move::new(
        Position::new(35, 35).unwrap(),
        Position::new(34, 34).unwrap(),
    ));
    s.push_repetition_key();
    let hash = s.hash();
    let repetitions = s.repetition_count();
    let history = serde_json::to_value(s.get_move_history()).unwrap();
    for (a, l) in [(false, false), (true, false), (false, true)] {
        let weights = w();
        let _m = set_modes(a, l);
        s.ensure_eval_inc(&weights);
        assert_eq!(
            evaluate_with_ply(&s, &weights, 0),
            evaluate_absolute_black(s.get_board(), &weights, 0)
                * if s.get_current_turn() == Color::Black {
                    1
                } else {
                    -1
                }
        );
    }
    assert_eq!(
        mate_probe(&s, Duration::from_millis(2), 64).status,
        ProbeStatus::NotApplicable
    );
    assert_eq!(s.hash(), hash);
    assert_eq!(s.repetition_count(), repetitions);
    assert_eq!(serde_json::to_value(s.get_move_history()).unwrap(), history);
}

#[test]
fn checkpoint_modes_drive_search_and_survive_model_switching() {
    use crate::{alphabeta_player::AlphaBetaPlayer, search::search};
    let mut cp = EvalCheckpoint::seed("Lmate");
    cp.weights.noise_scale = 0.;
    cp.weights.last_royal_mode = LastRoyalMode::MateProbe;
    let player = AlphaBetaPlayer::from_checkpoint(cp.clone());
    let mut cfg = player.config().clone();
    cfg.depth = 1;
    cfg.max_time_ms = Some(2000);
    let state = mate_fixture();
    let before = BoardPosition::from_state(&state);
    let r = search(&state, player.weights(), &cfg);
    let probe = r.royal_probe.unwrap();
    assert_eq!(probe.status, ProbeStatus::Found, "{probe:?}");
    assert_eq!(r.best_move, probe.winning_check);
    assert!(r.score > 900_000);
    assert_eq!(BoardPosition::from_state(&state), before);
    cp.weights.last_royal_mode = LastRoyalMode::Legacy;
    let plain = search(&state, &cp.weights, &cfg);
    assert!(plain.royal_probe.is_none());
    assert_eq!(plain.royal_extensions, 0);
    cp.weights.last_royal_mode = LastRoyalMode::MateProbe;
    cfg.max_time_ms = Some(0);
    let timeout = search(&state, &cp.weights, &cfg);
    assert_eq!(timeout.completed_depth, 0);
    assert_eq!(timeout.royal_probe.unwrap().status, ProbeStatus::Unknown);
    assert_eq!(
        search(&state, &cp.weights, &player.config()).completed_depth,
        2
    );
}

#[test]
fn checkpoint_modes_preserve_full_and_incremental_eval_parity() {
    let mut s = kings();
    put(&mut s, Rook, Color::White, 0, 1);
    put(&mut s, HookMover, Color::Black, 35, 18);
    for mode in [
        LastRoyalMode::Legacy,
        LastRoyalMode::VerifiedFlights,
        LastRoyalMode::ScarceDefenses,
        LastRoyalMode::MateProbe,
    ] {
        for blocked in [false, true] {
            let mut weights = w();
            weights.last_royal_mode = mode;
            weights.two_mover_align_blocked = blocked;
            let cp: EvalCheckpoint = serde_json::from_value(serde_json::json!({
                "format_version":1,"name":"roundtrip","created_at":"test",
                "search_defaults":SearchDefaults::default(),"weights":weights,
            }))
            .unwrap();
            let mut full = s.clone();
            assert!(full.eval_inc().is_none());
            let expected = evaluate_with_ply(&full, &cp.weights, 0);
            full.ensure_eval_inc(&cp.weights);
            assert_eq!(evaluate_with_ply(&full, &cp.weights, 0), expected);
        }
    }
}
