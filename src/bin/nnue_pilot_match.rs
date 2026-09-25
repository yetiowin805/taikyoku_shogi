//! Bounded paired-screen game from a complete replay prefix. Caps are unresolved, not draws.
use serde::Deserialize;
use serde_json::json;
use std::fs;
use std::io::{self, Write};
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::EvalCheckpoint,
    game_history::{GameHistory, GameResult},
    piece::Color,
    search::search,
    training::{
        record::{AgentSpec, GameRecordV2},
        worker::replay_to_ply,
    },
};

#[derive(Deserialize)]
struct Job {
    game: String,
    ply: usize,
    candidate: String,
    parent: String,
    candidate_black: bool,
    time_ms: u64,
    max_plies: usize,
    out: String,
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    let path = args.get(1).ok_or("usage: nnue_pilot_match JOB.json")?;
    let job: Job = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if job.time_ms == 0 || job.max_plies == 0 {
        return Err("nonpositive budget".into());
    }
    let mut record = GameRecordV2::load_path(std::path::Path::new(&job.game))?;
    if job.ply >= record.moves.len() {
        return Err("prefix exceeds source game".into());
    }
    let mut state = replay_to_ply(&record, job.ply)?;
    let cp_a = EvalCheckpoint::load_path(&job.candidate)?;
    let cp_b = EvalCheckpoint::load_path(&job.parent)?;
    if serde_json::to_value(&cp_a.search_defaults).map_err(|e| e.to_string())?
        != serde_json::to_value(&cp_b.search_defaults).map_err(|e| e.to_string())?
    {
        return Err("search settings differ".into());
    }
    let a = AlphaBetaPlayer::from_checkpoint(cp_a);
    let b = AlphaBetaPlayer::from_checkpoint(cp_b);
    let mut cfg = a.config().clone();
    let mut other = b.config().clone();
    cfg.depth = 8;
    cfg.max_time_ms = Some(job.time_ms);
    other.depth = 8;
    other.max_time_ms = Some(job.time_ms);
    record.moves.truncate(job.ply);
    record.result = None;
    record.abort_reason = None;
    let agent = |model: &str| AgentSpec {
        name: "ab".into(),
        model: Some(model.into()),
        depth: Some(8),
        max_time_ms: Some(job.time_ms),
        quiescence_depth: None,
        engine: None,
    };
    record.black = agent(if job.candidate_black {
        &job.candidate
    } else {
        &job.parent
    });
    record.white = agent(if job.candidate_black {
        &job.parent
    } else {
        &job.candidate
    });
    println!(
        "{}",
        json!({"event":"start","source_game":job.game,"prefix_plies":job.ply,
                          "candidate":job.candidate,"parent":job.parent,"candidate_black":job.candidate_black})
    );
    let start = Instant::now();
    let mut played = 0;
    let reason;
    loop {
        if state.is_draw_by_progress_rule()
            || state.is_draw_by_fivefold_repetition()
            || state.is_draw_by_insufficient_material()
        {
            record.result = Some(GameResult::Draw);
            reason = "rules_draw";
            break;
        }
        if let Some(winner) = state.get_winner() {
            record.result = Some(if winner == Color::Black {
                GameResult::BlackWins
            } else {
                GameResult::WhiteWins
            });
            reason = "royal_loss";
            break;
        }
        let legal = state.generate_legal_moves();
        if legal.is_empty() {
            record.result = Some(if state.get_current_turn() == Color::Black {
                GameResult::WhiteWins
            } else {
                GameResult::BlackWins
            });
            reason = "no_legal_move";
            break;
        }
        if played == job.max_plies {
            reason = "unresolved_ply_cap";
            record.abort_reason = Some(reason.into());
            break;
        }
        let color = state.get_current_turn();
        let is_candidate = (color == Color::Black) == job.candidate_black;
        let player = if is_candidate { &a } else { &b };
        let config = if is_candidate { &cfg } else { &other };
        let before = Instant::now();
        let r = search(&state, player.weights(), config);
        let mv = r.best_move.ok_or("search returned no move")?;
        if !legal.contains(&mv) {
            return Err("search returned illegal route".into());
        }
        let sign = if color == Color::Black { 1 } else { -1 };
        let mut m = GameHistory::move_to_record_with_eval(
            &mv,
            color,
            job.ply + played + 1,
            Some(r.score * sign),
            None,
            Some(r.nodes),
        );
        m.completed_depth = Some(r.completed_depth);
        state.make_move(mv);
        if state.get_current_turn() == color {
            return Err("move failed".into());
        }
        record.moves.push(m.clone());
        played += 1;
        println!(
            "{}",
            json!({"event":"move","played":played,"candidate":is_candidate,
                              "elapsed_ms":before.elapsed().as_millis(),"move":m})
        );
        io::stdout().flush().map_err(|e| e.to_string())?;
    }
    // The start event identifies the source players' prefix; model fields describe
    // the continuation. These experiment records never enter the training corpus.
    record.stats.move_count = record.moves.len();
    record.stats.elapsed_ms = Some(start.elapsed().as_millis() as u64);
    record.save_path(std::path::Path::new(&job.out))?;
    let score = match record.result {
        Some(GameResult::Draw) => Some(0.5),
        Some(GameResult::BlackWins) => Some(if job.candidate_black { 1.0 } else { 0.0 }),
        Some(GameResult::WhiteWins) => Some(if job.candidate_black { 0.0 } else { 1.0 }),
        None => None,
    };
    println!(
        "{}",
        json!({"event":"complete","candidate_score":score,"reason":reason,
                          "played":played,"elapsed_ms":start.elapsed().as_millis()})
    );
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("nnue_pilot_match: {e}");
        std::process::exit(1);
    }
}
