//! Isolated feature-gated tactical experiment CLI. One process per configuration.
use std::io::{self, Write};
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    game_history::GameHistory,
    game_state::GameState,
    notation::move_encode,
    piece::Color,
    player::AgentOptions,
    search::search_with_progress,
    training::record::{GameRecordV2, GameStart},
};

fn run() -> Result<(), String> {
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 3 && a[1] == "--validate-model" {
        taikyoku_shogi::eval::EvalCheckpoint::load_path(&a[2])?;
        return Ok(());
    }
    let allow_historical = a.len() == 7 && a[6] == "--allow-historical";
    if a.len() != 6 && !allow_historical {
        return Err("usage: tactical_probe GAME PLY MODEL DEPTH TIME_MS [--allow-historical]".into());
    }
    let tactical = taikyoku_shogi::search::TacticalOptions::from_env()?;
    let tempo_percent = taikyoku_shogi::eval::tactical_tempo_percent()?;
    let rec: GameRecordV2 =
        serde_json::from_slice(&std::fs::read(&a[1]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let ply: usize = a[2].parse().map_err(|_| "invalid ply")?;
    if ply == 0 || ply > rec.moves.len() {
        return Err("ply out of bounds".into());
    }
    let mut state = match &rec.start {
        GameStart::Opening => {
            let mut s = GameState::new();
            s.setup_initial_position();
            s
        }
        GameStart::Position { position } => position.to_state(),
    };
    for mv in &rec.moves[..ply - 1] {
        if state.get_current_turn() != mv.color {
            return Err("replay color mismatch".into());
        }
        let turn = state.get_current_turn();
        state.make_move(GameHistory::record_to_move(mv)?);
        if state.get_current_turn() == turn {
            return Err("replay failed".into());
        }
    }
    let color = state.get_current_turn();
    if color != rec.moves[ply - 1].color {
        return Err("target color mismatch".into());
    }
    let agent = if color == Color::Black {
        &rec.black
    } else {
        &rec.white
    };
    // The supervisor binds the matching content-pinned historical helper.
    // An explicit flag prevents accidental historical analysis with a current CLI.
    if agent.name != "ab" || (agent.engine.is_some() && !allow_historical) {
        return Err(
            "historical analysis requires its matching helper and --allow-historical"
                .into(),
        );
    }
    // Validate explicitly: from_options otherwise silently falls back to seed.
    taikyoku_shogi::eval::EvalCheckpoint::load_path(&a[3])?;
    let opts = AgentOptions {
        model: Some(a[3].clone()),
        depth: Some(a[4].parse().map_err(|_| "invalid depth")?),
        max_time_ms: Some(a[5].parse().map_err(|_| "invalid time")?),
        quiescence_depth: agent.quiescence_depth,
        cpu_percent: None,
        cancel: None,
    };
    let player = AlphaBetaPlayer::from_options(&opts);
    let mut config = player.config().clone();
    config.tactical = tactical;
    if let Ok(q) = std::env::var("TACTICAL_QDEPTH") {
        config.quiescence_depth = q.parse().map_err(|_| "invalid TACTICAL_QDEPTH")?;
    }
    if let Ok(value) = std::env::var("TACTICAL_QBROAD") {
        if value != "0" && value != "1" { return Err("TACTICAL_QBROAD must be 0 or 1".into()); }
        if value == "1" {
            config.q_own_large_only = false;
            config.q_open_any_capture = true;
            config.q_prune_mode = taikyoku_shogi::search::QPruneMode::Baseline;
        }
    }
    if let Ok(routes) = std::env::var("TACTICAL_ROOT") {
        let legal = state.generate_legal_moves();
        for route in routes.split(';') {
            let count = legal.iter().filter(|mv| move_encode(mv) == route).count();
            if count != 1 { return Err(format!("root route {route:?} matches {count} legal moves")); }
        }
    }
    let sign = if color == Color::Black { 1 } else { -1 };
    println!("{}", serde_json::json!({"event":"config", "ply":ply,
        "side_to_move":format!("{:?}",color),"board_hash":state.hash().to_string(),
        "tempo_percent":tempo_percent,"config":format!("{:?}",config)}));
    let start = Instant::now();
    let mut emit = |depth, score, mv: &taikyoku_shogi::game_state::Move, nodes, lines: &[(taikyoku_shogi::game_state::Move, i32)]| {
        println!(
            "{}",
            serde_json::json!({
                "event":"iteration", "completed_depth": depth, "score": score * sign, "best_move": move_encode(mv),
                "root_lines": lines.iter().map(|(m,s)| serde_json::json!({"move":move_encode(m),"score":s*sign})).collect::<Vec<_>>(),
                "nodes": nodes, "elapsed_ms": start.elapsed().as_millis()
            })
        );
        let _ = io::stdout().flush();
    };
    let result = search_with_progress(&state, player.weights(), &config, &mut emit);
    println!("{}", serde_json::json!({
        "event":"complete", "completed_depth":result.completed_depth,
        "target_depth": config.depth, "aborted": result.aborted,
        "terminal":result.best_move.is_none() && !result.aborted,
        "score":result.score*sign,"best_move":result.best_move.as_ref().map(move_encode),
        "elapsed_ms":start.elapsed().as_millis(),"nodes":result.nodes,"q_nodes":result.q_nodes,
        "q_caps_generated":result.q_caps_generated,"q_caps_searched":result.q_caps_searched,
        "q_tt_hits":result.q_tt_hits,"q_tt_probes":result.q_tt_probes,
        "royal_extensions":result.royal_extensions,"tactical":result.tactical_stats,
        "root_lines":result.root_lines.iter().map(|(m,s)|serde_json::json!({"move":move_encode(m),"score":s*sign})).collect::<Vec<_>>(),
        "iteration_timings":result.iteration_timings.iter().map(|i|serde_json::json!({"depth":i.depth,"elapsed_us":i.elapsed_us,"completed":i.completed})).collect::<Vec<_>>()
    }));
    if result.best_move.is_none() && !result.aborted {
        return Ok(());
    }
    if result.completed_depth == 0 {
        return Err("no search iteration completed".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("tactical_probe: {e}");
        std::process::exit(1);
    }
}
