//! Replay a complete game prefix and emit fixed-depth/timed search evidence.
use serde_json::json;
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::{evaluate, EvalCheckpoint},
    search::search,
    training::{record::load_game_json, worker::replay_to_ply},
};

fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("MODEL GAME PLY MILLISECONDS DEPTH".into());
    }
    let cp = EvalCheckpoint::load_path(&args[1])?;
    let record = load_game_json(&std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?)?;
    let ply = args[3].parse().map_err(|_| "bad ply")?;
    if ply > record.moves.len() {
        return Err("ply exceeds game length".into());
    }
    let mut state = replay_to_ply(&record, ply)?;
    state.ensure_eval_inc(&cp.weights);
    let static_score = evaluate(&state, &cp.weights);
    let mut cfg = AlphaBetaPlayer::from_checkpoint(cp.clone())
        .config()
        .clone();
    cfg.depth = args[5].parse().map_err(|_| "bad depth")?;
    cfg.max_time_ms = Some(args[4].parse().map_err(|_| "bad time")?);
    // Same untimed warmup as the post-merge ablation; reuse empty TT storage.
    let mut warm = cfg.clone();
    warm.depth = 1;
    warm.max_time_ms = Some(30000);
    let warmed = search(&state, &cp.weights, &warm);
    if warmed.aborted || warmed.completed_depth != 1 {
        return Err("warmup incomplete".into());
    }
    let start = Instant::now();
    let result = search(&state, &cp.weights, &cfg);
    let elapsed_ns = start.elapsed().as_nanos();
    let legal = result
        .best_move
        .as_ref()
        .is_some_and(|m| state.generate_legal_moves().contains(m));
    println!(
        "{}",
        json!({"elapsed_ns":elapsed_ns,"depth":result.completed_depth,
        "nodes":result.nodes,"qnodes":result.q_nodes,"score":result.score,"static_score":static_score,
        "best":result.best_move,"root_lines":result.root_lines,"legal":legal,"aborted":result.aborted})
    );
    Ok(())
}
