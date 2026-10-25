//! Isolated JSONL oracle for the tactical-tree pilot. Production search is unchanged.
//! Usage: tactical_tree_worker GAME START_PLY MODEL; stdin requests have version/id/op/path.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::{evaluate, EvalCheckpoint},
    game_history::GameHistory,
    game_state::{GameState, Move},
    notation::move_encode,
    piece::Color,
    search::{move_captures_enemy, search_with_progress},
    training::record::{GameRecordV2, GameStart},
};

#[derive(Deserialize)]
struct Request {
    version: u32,
    id: String,
    op: String,
    #[serde(default)]
    path: Vec<Move>,
    #[serde(default)]
    depth: u32,
    #[serde(default)]
    time_ms: u64,
    reply: Option<Move>,
}

fn emit(value: Value) {
    println!("{value}");
    let _ = io::stdout().flush();
}

fn sign(state: &GameState) -> i32 {
    if state.get_current_turn() == Color::Black {
        1
    } else {
        -1
    }
}

/// Follow tournament adjudication, not search's deliberately earlier repetition draw.
fn terminal(state: &GameState, no_moves: bool) -> Option<i32> {
    if state.is_draw_by_progress_rule()
        || state.is_draw_by_fivefold_repetition()
        || state.is_draw_by_insufficient_material()
    {
        Some(0)
    } else if let Some(winner) = state.get_winner() {
        Some(if winner == Color::Black { 1 } else { -1 })
    } else if no_moves {
        Some(-sign(state))
    } else {
        None
    }
}

fn legal(state: &GameState) -> Vec<Move> {
    let mut seen = HashSet::new();
    state
        .generate_legal_moves()
        .into_iter()
        .filter(|mv| {
            // Full route *and* MoveData identity; endpoint or display-token dedup is unsafe.
            seen.insert(serde_json::to_string(mv).unwrap())
        })
        .collect()
}

fn advance(state: &mut GameState, mv: &Move) -> Result<(), String> {
    if terminal(state, false).is_some() || !legal(state).contains(mv) {
        return Err(format!("illegal continuation {}", move_encode(mv)));
    }
    let old = state.get_current_turn();
    state.make_move(mv.clone());
    if state.get_current_turn() == old {
        return Err("move application failed".into());
    }
    Ok(())
}

fn position(base: &GameState, path: &[Move]) -> Result<GameState, String> {
    let mut state = base.clone();
    for mv in path {
        advance(&mut state, mv)?;
    }
    Ok(state)
}

fn handle(base: &GameState, player: &AlphaBetaPlayer, req: &Request) -> Result<(), String> {
    if req.version != 1 {
        return Err("unsupported protocol version".into());
    }
    let started = Instant::now();
    let state = position(base, &req.path)?;
    let pov = sign(&state);
    if req.op == "inspect" {
        let moves = legal(&state);
        let outcome = terminal(&state, moves.is_empty());
        let mut reply = Value::Null;
        if let Some(mv) = &req.reply {
            let valid = outcome.is_none() && moves.contains(mv);
            reply = json!({"legal":valid});
            if valid {
                let mut after = state.clone();
                advance(&mut after, mv)?;
                reply["static_black"] = json!(evaluate(&after, player.weights()) * sign(&after));
            }
        }
        emit(
            json!({"version":1,"id":req.id,"event":"complete","op":"inspect",
            "turn":pov,"static_black":evaluate(&state,player.weights()) * pov,
            "terminal":outcome,"draw_counter":state.get_turns_without_capture_or_promotion(),
            "repetitions":state.repetition_count(),"history_len":state.get_move_history().len(),
            "position_hash":format!("{:016x}",state.repetition_key()),"reply":reply,
            "legal": if outcome.is_some() { vec![] } else { moves.into_iter().map(|mv| {
                json!({"move":mv,"label":move_encode(&mv),"capture":move_captures_enemy(&state,&mv)})
            }).collect::<Vec<_>>() },"elapsed_ms":started.elapsed().as_millis()}),
        );
    } else if req.op == "search" {
        if req.depth == 0 || req.time_ms == 0 {
            return Err("positive depth/time_ms required".into());
        }
        let moves = legal(&state);
        if let Some(outcome) = terminal(&state, moves.is_empty()) {
            emit(
                json!({"version":1,"id":req.id,"event":"complete","op":"search",
                "terminal":outcome,"completed_depth":0,"nodes":0,"elapsed_ms":started.elapsed().as_millis()}),
            );
            return Ok(());
        }
        let mut config = player.config().clone();
        config.depth = req.depth;
        config.max_time_ms = Some(req.time_ms);
        config.fischer_soft_ms = None;
        config.collect_trace = false;
        let mut progress = |depth, score, mv: &Move, nodes, lines: &[(Move, i32)]| {
            let mut ranked = lines.to_vec();
            ranked.sort_by_key(|entry| std::cmp::Reverse(entry.1));
            emit(
                json!({"version":1,"id":req.id,"event":"iteration","depth":depth,
                "score_black":score*pov,"best":mv,"nodes":nodes,
                "elapsed_ms":started.elapsed().as_millis(),
                "candidates":ranked.iter().take(12).map(|(m,s)|json!({"move":m,"score_black":s*pov})).collect::<Vec<_>>(),
                "candidate_scores_are_heuristic":true}),
            );
        };
        let result = search_with_progress(&state, player.weights(), &config, &mut progress);
        emit(
            json!({"version":1,"id":req.id,"event":"complete","op":"search",
            "completed_depth":result.completed_depth,"nodes":result.nodes,"q_nodes":result.q_nodes,
            "aborted":result.aborted,"elapsed_ms":started.elapsed().as_millis(),"timings":result.iteration_timings}),
        );
    } else {
        return Err("unknown operation".into());
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: tactical_tree_worker GAME START_PLY MODEL".into());
    }
    let game = GameRecordV2::load_path(std::path::Path::new(&args[1]))?;
    let ply: usize = args[2].parse().map_err(|_| "invalid start ply")?;
    if ply == 0 || ply > game.moves.len() + 1 {
        return Err("start ply out of range".into());
    }
    let mut base = match &game.start {
        GameStart::Opening => {
            let mut s = GameState::new();
            s.setup_initial_position();
            s
        }
        GameStart::Position { position } => position.to_state()?,
    };
    for record in &game.moves[..ply - 1] {
        if record.color != base.get_current_turn() {
            return Err("prefix color mismatch".into());
        }
        // Original immutable records follow the same complete-history replay as analyze_position.
        let old = base.get_current_turn();
        base.make_move(GameHistory::record_to_move(record)?);
        if base.get_current_turn() == old {
            return Err("prefix replay failed".into());
        }
    }
    let player = AlphaBetaPlayer::from_checkpoint(EvalCheckpoint::load_path(&args[3])?);
    let actual = game.moves[ply - 1..]
        .iter()
        .take(32)
        .map(GameHistory::record_to_move)
        .collect::<Result<Vec<_>, _>>()?;
    emit(
        json!({"version":1,"event":"ready","actual":actual,"history_len":base.get_move_history().len(),
        "search_config":format!("{:?}",player.config())}),
    );
    for line in io::stdin().lock().lines() {
        let line = line.map_err(|e| e.to_string())?;
        let req: Request = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        if let Err(error) = handle(&base, &player, &req) {
            emit(json!({"version":1,"id":req.id,"event":"error","error":error}));
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("tactical_tree_worker: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use taikyoku_shogi::{piece::PieceType, position::Position};
    #[test]
    fn original_history_and_rejected_route_are_preserved() {
        let mut s = GameState::new();
        s.setup_initial_position();
        let mv = legal(&s).remove(0);
        let child = position(&s, std::slice::from_ref(&mv)).unwrap();
        assert_eq!(child.get_move_history().len(), 1);
        assert_eq!(s.get_move_history().len(), 0);
        let bad = Move::new(mv.from, mv.from);
        assert!(position(&s, &[bad]).is_err());
    }
    #[test]
    fn search_repetition_is_not_a_proof_of_a_game_draw() {
        let mut s = GameState::new();
        s.setup_initial_position();
        s.push_repetition_key();
        assert!(s.is_repetition_draw_for_search());
        assert_eq!(terminal(&s, false), None);
        for _ in 0..3 {
            s.push_repetition_key();
        }
        assert_eq!(terminal(&s, false), Some(0));
    }
    #[test]
    fn actual_royal_capture_is_proven() {
        let mut s = GameState::new();
        s.clear_board();
        s.place_piece(taikyoku_shogi::piece::Piece::new(
            PieceType::King,
            Color::Black,
            Position::new(0, 0).unwrap(),
        ));
        assert_eq!(terminal(&s, false), Some(1));
    }
}
