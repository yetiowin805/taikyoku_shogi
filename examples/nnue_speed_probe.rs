//! Local-only timing harness; loading/replay are outside the search timer.
use serde_json::json;
use std::{hint::black_box, time::Instant};
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    eval::{bind_search_weights, evaluate, EvalCheckpoint},
    nnue::Accumulator,
    piece::Color,
    search::search,
    training::{record::load_game_json, worker::replay_to_ply},
};
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 7 {
        return Err("MODEL GAME PLY MILLISECONDS DEPTH search|micro".into());
    }
    let cp = EvalCheckpoint::load_path(&args[1])?;
    let rec = load_game_json(&std::fs::read_to_string(&args[2]).map_err(|e| e.to_string())?)?;
    let mut state = replay_to_ply(&rec, args[3].parse().map_err(|_| "bad ply")?)?;
    state.ensure_eval_inc(&cp.weights);
    let static_score = evaluate(&state, &cp.weights);
    if args[6] == "micro" {
        let net = cp
            .weights
            .nnue_runtime
            .clone()
            .ok_or("micro requires NNUE")?;
        let mut acc = Accumulator::new(net, state.get_board());
        let pieces: Vec<_> = [Color::Black, Color::White]
            .into_iter()
            .flat_map(|c| state.get_board().pieces_by_color(c).iter().copied())
            .take(64)
            .collect();
        let moves: Vec<_> = state.generate_legal_moves().into_iter().take(32).collect();
        let _binding = bind_search_weights(&cp.weights);
        let mut samples = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            for _ in 0..2000 {
                black_box(acc.residual(black_box(Color::Black)));
            }
            let forward_ns = t.elapsed().as_nanos() as f64 / 2000.;
            let t = Instant::now();
            for _ in 0..64 {
                for p in &pieces {
                    acc.change(black_box(p), -1);
                    acc.change(black_box(p), 1);
                }
            }
            black_box(&acc);
            let change_pair_ns = t.elapsed().as_nanos() as f64 / (64 * pieces.len()) as f64;
            let t = Instant::now();
            for _ in 0..16 {
                for m in &moves {
                    let undo = state
                        .make_move_for_search(black_box(m.clone()))
                        .ok_or("make failed")?;
                    state.unmake_move_for_search(undo);
                }
            }
            let make_unmake_ns = t.elapsed().as_nanos() as f64 / (16 * moves.len()) as f64;
            assert!(acc.matches_rebuild(state.get_board()));
            assert_eq!(static_score, evaluate(&state, &cp.weights));
            samples.push(json!({"forward_ns":forward_ns,"change_pair_ns":change_pair_ns,"make_unmake_ns":make_unmake_ns}));
        }
        println!("{}", json!({"micro":samples}));
    } else {
        assert_eq!(args[6], "search");
        let mut cfg = AlphaBetaPlayer::from_checkpoint(cp.clone())
            .config()
            .clone();
        cfg.depth = args[5].parse().map_err(|_| "bad depth")?;
        cfg.max_time_ms = Some(args[4].parse().map_err(|_| "bad time")?);
        let t = Instant::now();
        let r = search(&state, &cp.weights, &cfg);
        let elapsed_ns = t.elapsed().as_nanos();
        let legal = r
            .best_move
            .as_ref()
            .is_some_and(|m| state.generate_legal_moves().contains(m));
        println!(
            "{}",
            json!({"elapsed_ns":elapsed_ns,"depth":r.completed_depth,
            "nodes":r.nodes,"qnodes":r.q_nodes,"score":r.score,"static_score":static_score,
            "best":r.best_move,"root_lines":r.root_lines,"legal":legal,"aborted":r.aborted})
        );
    }
    Ok(())
}
