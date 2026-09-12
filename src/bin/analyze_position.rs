//! One position per process; JSONL iteration results survive a watchdog timeout.
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
    if a.len() != 6 {
        return Err("usage: analyze_position GAME PLY MODEL DEPTH TIME_MS".into());
    }
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
    if agent.name != "ab" || agent.engine.is_some() {
        return Err(
            "analysis requires current in-process ab agent; historical engines are unsupported"
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
    };
    let player = AlphaBetaPlayer::from_options(&opts);
    let sign = if color == Color::Black { 1 } else { -1 };
    let start = Instant::now();
    let mut emit = |depth, score, mv: &taikyoku_shogi::game_state::Move, nodes| {
        println!(
            "{}",
            serde_json::json!({
                "completed_depth": depth, "score": score * sign, "best_move": move_encode(mv),
                "nodes": nodes, "elapsed_ms": start.elapsed().as_millis()
            })
        );
        let _ = io::stdout().flush();
    };
    let result = search_with_progress(&state, player.weights(), player.config(), &mut emit);
    if result.best_move.is_none() && !result.aborted {
        println!(
            "{}",
            serde_json::json!({
                "completed_depth": 0, "terminal": true, "score": result.score * sign,
                "best_move": null, "nodes": result.nodes, "elapsed_ms": start.elapsed().as_millis()
            })
        );
        return Ok(());
    }
    if result.completed_depth == 0 {
        return Err("no search iteration completed".into());
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("analyze_position: {e}");
        std::process::exit(1);
    }
}
