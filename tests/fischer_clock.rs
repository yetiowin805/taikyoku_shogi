use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use taikyoku_shogi::{
    board_position::BoardPosition,
    eval::EvalWeights,
    game_state::GameState,
    piece::{Color, Piece, PieceType},
    position::Position,
    search::{search_with_progress, SearchConfig},
    training::{
        clock::FischerControl,
        record::{AgentSpec, GameRecordV2},
        worker::{play_one_game, start_from_position, WorkerConfig},
    },
};

fn position() -> GameState {
    let mut state = GameState::new();
    for (kind, color, x, y) in [
        (PieceType::King, Color::Black, 5, 5),
        (PieceType::GoldGeneral, Color::Black, 7, 7),
        (PieceType::King, Color::White, 30, 30),
        (PieceType::GoldGeneral, Color::White, 28, 28),
    ] {
        state.place_piece(Piece::new(kind, color, Position::new(x, y).unwrap()));
    }
    state.recompute_hash();
    state.reset_rep_history();
    state
}

#[test]
fn fischer_search_one_extension_and_last_completed_iteration() {
    let state = position();
    let weights = EvalWeights::seed();
    let mut cfg = SearchConfig {
        depth: 8,
        max_time_ms: Some(30_000),
        fischer_soft_ms: Some(0),
        quiescence_depth: 0,
        ..Default::default()
    };
    let mut iterations = vec![];
    let result = search_with_progress(&state, &weights, &cfg, &mut |d, _, _, _, _| {
        iterations.push(d)
    });
    // Depth one is mandatory; a zero soft budget admits exactly one more depth.
    assert_eq!(iterations, vec![1, 2]);
    assert_eq!(result.completed_depth, 2);
    let cancel = Arc::new(AtomicBool::new(false));
    cfg.cancel = Some(cancel.clone());
    let mut completed = None;
    let result = search_with_progress(&state, &weights, &cfg, &mut |d, s, m, _, _| {
        completed = Some((d, s, m.clone()));
        cancel.store(true, Ordering::Relaxed);
    });
    let (depth, score, mv) = completed.unwrap();
    assert_eq!(result.completed_depth, depth);
    assert_eq!(result.score, score);
    assert_eq!(result.best_move, Some(mv));
    cfg.cancel = None;
    cfg.max_time_ms = Some(500);
    let mut completed = None;
    let expired = search_with_progress(&state, &weights, &cfg, &mut |d, s, m, _, _| {
        completed = Some((d, s, m.clone()));
        std::thread::sleep(std::time::Duration::from_millis(500));
    });
    let (depth, score, mv) = completed.unwrap();
    assert_eq!(
        (expired.completed_depth, expired.score, expired.best_move),
        (depth, score, Some(mv))
    );
    cfg.max_time_ms = Some(0);
    let expired = search_with_progress(&state, &weights, &cfg, &mut |_, _, _, _, _| {
        panic!("expired search completed")
    });
    assert_eq!(expired.completed_depth, 0);
    assert!(expired.best_move.is_some());
}

#[test]
fn clocks_are_independent_recorded_and_round_trip() {
    let clock = FischerControl {
        initial_ms: 60_000,
        increment_ms: 5000,
    };
    let cfg = WorkerConfig {
        black: AgentSpec::new("random"),
        white: AgentSpec::new("random"),
        start: start_from_position(BoardPosition::from_state(&position())),
        max_moves: 4,
        time_control: Some(clock),
        ..Default::default()
    };
    let record = play_one_game(&cfg).unwrap();
    assert_eq!(record.moves.len(), 4);
    let timing = record.stats.clock.as_ref().unwrap();
    assert_eq!(timing.moves.len(), record.moves.len());
    let mut remaining = [clock.initial_ms; 2];
    for (i, mv) in timing.moves.iter().enumerate() {
        let side = i % 2;
        remaining[side] = clock.finish_move(remaining[side], mv.elapsed_ms).unwrap();
        assert_eq!(mv.remaining_ms, remaining[side]);
    }
    let decoded: GameRecordV2 =
        serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(decoded.stats.clock, record.stats.clock);
    let mut stopped = cfg.clone();
    stopped.stop = Some(Arc::new(AtomicBool::new(true)));
    let partial = play_one_game(&stopped).unwrap_err().partial;
    assert_eq!(partial.stats.clock.unwrap().control, clock);
    let mut historical = cfg;
    historical.black.engine = Some("must-not-start".into());
    assert!(play_one_game(&historical)
        .unwrap_err()
        .message
        .contains("historical"));
}

#[test]
fn rolling_worker_retains_iteration_sidecar_after_parent_consumes_result() {
    use taikyoku_shogi::training::game_process::{self, EngineBundle};
    let dir = std::env::temp_dir().join(format!("iteration-sidecar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let binary = env!("CARGO_BIN_EXE_taikyoku_shogi");
    let hash = game_process::file_hash(std::path::Path::new(binary)).unwrap();
    let bundle = EngineBundle { protocol: 1, revision: "test".into(),
        engine_bin: binary.into(), engine_sha256: hash.clone(),
        analyzer_bin: binary.into(), analyzer_sha256: hash };
    let pointer = dir.join("active.json");
    std::fs::write(&pointer, serde_json::to_vec(&bundle).unwrap()).unwrap();
    let mut agent = AgentSpec::new("ab");
    agent.depth = Some(2);
    agent.quiescence_depth = Some(0);
    let cfg = WorkerConfig { black: agent.clone(), white: agent,
        start: start_from_position(BoardPosition::from_state(&position())),
        max_moves: 2, ..Default::default() };
    let record = game_process::play(&cfg, &pointer, &dir, 1).unwrap();
    assert!(!dir.join("slot-1.result.json").exists());
    let sidecar: serde_json::Value = serde_json::from_slice(
        &std::fs::read(dir.join("slot-1.result.iterations.json")).unwrap()).unwrap();
    assert_eq!(sidecar["game_id"], record.game_id);
    assert_eq!(sidecar["moves"].as_array().unwrap().len(), record.moves.len());
    for m in &record.moves {
        assert!(!m.iteration_timings.is_empty());
        assert!(m.iteration_timings.iter().all(|t| t.completed));
    }
    let encoded = serde_json::to_value(&record).unwrap();
    let decoded: GameRecordV2 = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
    std::fs::remove_dir_all(dir).unwrap();
}
