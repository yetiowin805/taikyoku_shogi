//! Isolated JSONL oracle for the tactical-tree pilot. Production search is unchanged.
//! Usage: tactical_tree_worker GAME START_PLY MODEL; stdin requests have version/id/op/path.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};
use std::time::Instant;
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::{evaluate, material_piece_value, EvalCheckpoint, EvalWeights},
    game_history::GameHistory,
    game_state::{GameState, Move},
    movement::{BlockingMode, MovementCapability, MovementConfig},
    notation::move_encode,
    piece::{Color, Piece},
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
    /// Optional checkpoint for this request only; permits paired trial policies.
    model: Option<String>,
    /// Absolute, one-based root plies in the original game, for `scan`.
    #[serde(default)]
    plies: Vec<usize>,
    from_ply: Option<usize>,
    to_ply: Option<usize>,
    /// Attack probes for royals and a bounded set of major pieces.
    #[serde(default)]
    features: bool,
    /// Count legal routes for the probed majors, on both turns. More expensive.
    #[serde(default)]
    mobility: bool,
    #[serde(default)]
    include_pieces: bool,
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

fn has_capturing_range(caps: &[MovementCapability]) -> bool {
    caps.iter().any(|cap| match cap {
        MovementCapability::Range { blocking, .. } => *blocking == BlockingMode::Capturing,
        MovementCapability::TwoStep { first, second } => {
            has_capturing_range(std::slice::from_ref(first.as_ref()))
                || has_capturing_range(std::slice::from_ref(second.as_ref()))
        }
        _ => false,
    })
}

fn piece_description(piece: &Piece, weights: &EvalWeights) -> Value {
    let caps = &MovementConfig::for_piece(piece).capabilities;
    json!({"piece":piece,"material_value":material_piece_value(piece,weights),
        "royal":piece.piece_type.is_royal(),"capturing_range":has_capturing_range(caps),
        "two_step":caps.iter().any(|cap|matches!(cap,MovementCapability::TwoStep{..})),
        "free_eagle":caps.iter().any(|cap|matches!(cap,MovementCapability::FreeEagleMultiMove{..}))})
}

fn pieces(state: &GameState) -> Vec<Piece> {
    let board = state.get_board();
    board
        .iter_pieces_by_color(Color::Black)
        .chain(board.iter_pieces_by_color(Color::White))
        .collect()
}

/// Compare actual boards, rather than guessing victims from an endpoint or ray.
fn move_description(
    before: &GameState,
    after: &GameState,
    mv: &Move,
    weights: &EvalWeights,
) -> Value {
    let removed: Vec<_> = pieces(before)
        .iter()
        .filter(|piece| {
            piece.position != mv.from
                && after.get_board().get_piece(piece.position) != Some(**piece)
        })
        .map(|piece| piece_description(piece, weights))
        .collect();
    json!({"move":mv,"label":move_encode(mv),
        "mover":before.get_board().get_piece(mv.from).map(|p|piece_description(&p,weights)),
        "landed":after.get_board().get_piece(mv.to).map(|p|piece_description(&p,weights)),
        "removed":removed})
}

fn position_features(state: &GameState, weights: &EvalWeights, mobility: bool) -> Value {
    let board = state.get_board();
    let all = pieces(state);
    let royals: Vec<_> = all
        .iter()
        .filter(|piece| piece.piece_type.is_royal())
        .map(|piece| {
            let mut info = piece_description(piece, weights);
            // Both probes are reported: the engine's last-royal check predicate
            // deliberately treats some capturing ranges differently.
            info["attacked"] =
                json!(board.is_position_attacked_by_color(piece.position, piece.color.opposite()));
            info["attacked_for_check"] = json!(board
                .is_position_attacked_by_color_for_check(piece.position, piece.color.opposite()));
            info
        })
        .collect();
    let mut majors: Vec<_> = all
        .iter()
        .filter(|piece| {
            let caps = &MovementConfig::for_piece(piece).capabilities;
            has_capturing_range(caps)
                || caps.iter().any(|cap| {
                    matches!(
                        cap,
                        MovementCapability::TwoStep { .. }
                            | MovementCapability::FreeEagleMultiMove { .. }
                    )
                })
        })
        .collect();
    majors.sort_by(|a, b| {
        material_piece_value(b, weights).total_cmp(&material_piece_value(a, weights))
    });
    let major_total = majors.len();
    majors.truncate(32);
    let route_counts = if mobility {
        let mut counts = std::collections::HashMap::new();
        for view in [state.clone(), state.with_opponent_turn()] {
            for mv in legal(&view) {
                *counts.entry(mv.from).or_insert(0usize) += 1;
            }
        }
        Some(counts)
    } else {
        None
    };
    let major_info: Vec<_> = majors
        .into_iter()
        .map(|piece| {
            let mut info = piece_description(piece, weights);
            info["attacked"] =
                json!(board.is_position_attacked_by_color(piece.position, piece.color.opposite()));
            if let Some(counts) = &route_counts {
                info["legal_routes"] = json!(counts.get(&piece.position).copied().unwrap_or(0));
            }
            info
        })
        .collect();
    json!({"black_pieces":board.iter_pieces_by_color(Color::Black).count(),
        "white_pieces":board.iter_pieces_by_color(Color::White).count(),
        "royals":royals,"majors":major_info,"major_probe_count":major_total.min(32),
        "major_total":major_total,"major_selection":"top 32 capturing-range or multi-leg pieces by material value",
        "mobility_counts_routes_not_safe_squares":true})
}

fn selected_plies(req: &Request, start_ply: usize, game_len: usize) -> Result<Vec<usize>, String> {
    if !req.plies.is_empty() && (req.from_ply.is_some() || req.to_ply.is_some()) {
        return Err("use either plies or from_ply/to_ply".into());
    }
    let mut plies = req.plies.clone();
    if plies.is_empty() {
        let from = req.from_ply.unwrap_or(start_ply);
        let to = req.to_ply.unwrap_or(from);
        if from > to || to.saturating_sub(from) >= 512 {
            return Err("scan range must be ordered and contain at most 512 root plies".into());
        }
        plies.extend(from..=to);
    }
    plies.sort_unstable();
    plies.dedup();
    if plies.len() > 512
        || plies
            .iter()
            .any(|&ply| ply < start_ply || ply > game_len + 1)
    {
        return Err("scan plies must be within the replay suffix and number at most 512".into());
    }
    Ok(plies)
}

fn scan(
    base: &GameState,
    start_ply: usize,
    game: &GameRecordV2,
    player: &AlphaBetaPlayer,
    req: &Request,
    started: Instant,
) -> Result<(), String> {
    if !req.path.is_empty() {
        return Err("scan uses the actual game suffix and does not accept path".into());
    }
    let selected = selected_plies(req, start_ply, game.moves.len())?;
    let last = *selected.last().ok_or("empty scan")?;
    let mut state = base.clone();
    let mut selected_iter = selected.iter().peekable();
    for ply in start_ply..=last {
        let record = game.moves.get(ply - 1);
        let wanted = selected_iter.peek().is_some_and(|&&p| p == ply);
        let mut info = if wanted {
            let static_black = evaluate(&state, player.weights()) * sign(&state);
            let mut info = json!({"version":1,"id":req.id,"event":"position","op":"scan",
                "ply":ply,"turn":sign(&state),"static_black":static_black,
                "recorded_static_black":record.and_then(|r|r.static_eval),
                "recorded_search_black":record.and_then(|r|r.eval),
                "recorded_depth":record.and_then(|r|r.completed_depth),
                "recorded_nodes":record.and_then(|r|r.nodes),
                "recorded_search_minus_static":record.and_then(|r|r.eval.zip(r.static_eval).map(|(e,s)|i64::from(e)-i64::from(s))),
                "recorded_minus_recomputed_static":record.and_then(|r|r.static_eval.map(|s|i64::from(s)-i64::from(static_black))),
                "draw_counter":state.get_turns_without_capture_or_promotion(),
                "repetitions":state.repetition_count(),"history_len":state.get_move_history().len(),
                "position_hash":format!("{:016x}",state.repetition_key()),
                "terminal_without_no_moves_probe":terminal(&state,false)});
            if req.features || req.mobility {
                info["features"] = position_features(&state, player.weights(), req.mobility);
            }
            if req.include_pieces {
                info["pieces"] = json!(pieces(&state));
            }
            Some(info)
        } else {
            None
        };
        if let Some(record) = record {
            if record.color != state.get_current_turn() {
                return Err(format!("replay color mismatch at ply {ply}"));
            }
            let mv = GameHistory::record_to_move(record)?;
            let before = wanted.then(|| state.clone());
            let old_turn = state.get_current_turn();
            // Trusted immutable game records: make_move validates the route while
            // retaining full history; generating every sibling is unnecessary.
            state.make_move(mv.clone());
            if state.get_current_turn() == old_turn {
                return Err(format!("replay failed at ply {ply}"));
            }
            if let (Some(info), Some(before)) = (&mut info, &before) {
                info["actual"] = move_description(before, &state, &mv, player.weights());
                info["after_static_black"] =
                    json!(evaluate(&state, player.weights()) * sign(&state));
                if req.features {
                    info["after_features"] = position_features(&state, player.weights(), false);
                }
            }
        }
        if let Some(info) = info {
            emit(info);
            selected_iter.next();
        }
    }
    emit(
        json!({"version":1,"id":req.id,"event":"complete","op":"scan",
        "positions":selected.len(),"elapsed_ms":started.elapsed().as_millis()}),
    );
    Ok(())
}

fn handle(
    base: &GameState,
    start_ply: usize,
    game: &GameRecordV2,
    default_player: &AlphaBetaPlayer,
    req: &Request,
) -> Result<(), String> {
    if req.version != 1 {
        return Err("unsupported protocol version".into());
    }
    let override_player = req
        .model
        .as_ref()
        .map(|path| EvalCheckpoint::load_path(path).map(AlphaBetaPlayer::from_checkpoint))
        .transpose()?;
    let player = override_player.as_ref().unwrap_or(default_player);
    // Checkpoint decoding is setup cost, outside the position/search measurement.
    let started = Instant::now();
    if req.op == "scan" {
        return scan(base, start_ply, game, player, req, started);
    }
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
        let mut info = json!({"version":1,"id":req.id,"event":"complete","op":"inspect",
            "turn":pov,"static_black":evaluate(&state,player.weights()) * pov,
            "terminal":outcome,"draw_counter":state.get_turns_without_capture_or_promotion(),
            "repetitions":state.repetition_count(),"history_len":state.get_move_history().len(),
            "position_hash":format!("{:016x}",state.repetition_key()),"reply":reply,
            "legal": if outcome.is_some() { vec![] } else { moves.into_iter().map(|mv| {
                json!({"move":mv,"label":move_encode(&mv),"capture":move_captures_enemy(&state,&mv)})
            }).collect::<Vec<_>>() },"elapsed_ms":started.elapsed().as_millis()});
        if req.features || req.mobility {
            info["features"] = position_features(&state, player.weights(), req.mobility);
        }
        if req.include_pieces {
            info["pieces"] = json!(pieces(&state));
        }
        emit(info);
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
            "partial_root_depth":result.partial_root_depth,
            "selected_depth":result.partial_root_depth.unwrap_or(result.completed_depth),
            "royal_probe":result.royal_probe,"royal_extensions":result.royal_extensions,
            "score_black":result.score*pov,"static_black":result.static_eval*pov,"best":result.best_move,
            "q_caps_generated":result.q_caps_generated,"q_caps_searched":result.q_caps_searched,
            "q_tt_hits":result.q_tt_hits,"q_tt_probes":result.q_tt_probes,
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
        if let Err(error) = handle(&base, ply, &game, &player, &req) {
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
    fn scan_selection_is_bounded_and_keeps_absolute_plies() {
        let req = |extra: Value| {
            let mut value = json!({"version":1,"id":"test","op":"scan"});
            value
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            serde_json::from_value::<Request>(value).unwrap()
        };
        assert_eq!(
            selected_plies(&req(json!({"plies":[9,7,9]})), 5, 10).unwrap(),
            vec![7, 9]
        );
        assert_eq!(
            selected_plies(&req(json!({"from_ply":10,"to_ply":11})), 5, 10).unwrap(),
            vec![10, 11]
        );
        assert!(selected_plies(&req(json!({"plies":[4]})), 5, 10).is_err());
        assert!(selected_plies(&req(json!({"from_ply":5,"to_ply":517})), 5, 600).is_err());
        assert!(selected_plies(&req(json!({"plies":[6],"to_ply":8})), 5, 10).is_err());
    }
    #[test]
    fn move_description_records_range_friend_and_enemy_losses() {
        let mut before = GameState::new();
        before.clear_board();
        for (pt, color, file, rank) in [
            (PieceType::King, Color::Black, 0, 0),
            (PieceType::King, Color::White, 35, 35),
            (PieceType::GreatGeneral, Color::Black, 5, 5),
            (PieceType::Pawn, Color::Black, 6, 5),
            (PieceType::Pawn, Color::White, 8, 5),
        ] {
            before.place_piece(Piece::new(pt, color, Position::new(file, rank).unwrap()));
        }
        let mv = Move::new(Position::new(5, 5).unwrap(), Position::new(9, 5).unwrap());
        let mut after = before.clone();
        advance(&mut after, &mv).unwrap();
        let description = move_description(&before, &after, &mv, &EvalWeights::default());
        let removed = description["removed"].as_array().unwrap();
        assert_eq!(removed.len(), 2);
        assert!(removed.iter().any(|p| p["piece"]["color"] == "Black"));
        assert!(removed.iter().any(|p| p["piece"]["color"] == "White"));
        assert_eq!(description["move"], json!(mv));
    }
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
