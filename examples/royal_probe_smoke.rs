//! Small, opt-in evaluation sanity/cost study. Never reads or writes live services.
use rand::{seq::SliceRandom, SeedableRng};
use serde::Deserialize;
use std::{
    hint::black_box,
    io::Write,
    time::{Duration, Instant},
};
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    board_position::BoardPosition,
    eval::{evaluate_with_ply, royal_probes::*, EvalCheckpoint, EvalWeights},
    game_history::GameHistory,
    game_state::GameState,
    piece::{Color, Piece, PieceType},
    position::Position,
    search::search,
    training::record::{GameRecordV2, GameStart},
};

#[derive(Deserialize)]
struct Case {
    name: String,
    game: String,
    ply: usize,
    model: String,
}
fn load(c: &Case) -> Result<(GameState, EvalCheckpoint), String> {
    let rec: GameRecordV2 =
        serde_json::from_slice(&std::fs::read(&c.game).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut s = match rec.start {
        GameStart::Opening => {
            let mut s = GameState::new();
            s.setup_initial_position();
            s
        }
        GameStart::Position { position } => position.to_state(),
    };
    if c.ply > rec.moves.len() {
        return Err("prefix out of range".into());
    }
    for m in &rec.moves[..c.ply] {
        let turn = s.get_current_turn();
        if turn != m.color {
            return Err("prefix color mismatch".into());
        }
        s.make_move(GameHistory::record_to_move(m)?);
        if s.get_current_turn() == turn {
            return Err("prefix replay failed".into());
        }
    }
    Ok((s, EvalCheckpoint::load_path(&c.model)?))
}
fn weights(base: &EvalWeights, variant: &str) -> EvalWeights {
    let mut w = base.clone();
    w.noise_scale = 0.;
    w.two_mover_align_k = 0.;
    w.two_mover_align_cap = 0.;
    w.lr_flight_k = 0.;
    let k = match variant {
        "A40" => 40.,
        "A80" | "Ablocked" => 80.,
        "A160" => 160.,
        _ => 0.,
    };
    w.two_mover_align_k = k;
    w.two_mover_align_cap = k * 5.;
    if variant.starts_with('L') {
        w.lr_flight_k = 4000.;
    }
    w
}
fn emit(v: serde_json::Value) {
    println!("{v}");
    std::io::stdout().flush().unwrap();
}
fn put(s: &mut GameState, t: PieceType, c: Color, x: u8, y: u8) {
    s.place_piece(Piece::new(t, c, Position::new(x, y).unwrap()));
}
fn fixtures() -> Vec<(String, GameState)> {
    use Color::*;
    use PieceType::*;
    let mut xs = Vec::new();
    for name in [
        "capture-escape",
        "xray-trap",
        "two-square-escape",
        "interposition",
        "incoming-mate",
        "quiet-box",
    ] {
        let mut s = GameState::new();
        s.clear_board();
        put(&mut s, King, Black, 0, 0);
        put(&mut s, King, White, 35, 35);
        match name {
            "capture-escape" => {
                put(&mut s, Rook, White, 0, 1);
                put(&mut s, Pawn, Black, 1, 0);
                put(&mut s, Pawn, Black, 1, 1);
            }
            "xray-trap" => {
                s.get_board_mut().remove_piece(Position::new(0, 0).unwrap());
                put(&mut s, King, Black, 0, 1);
                put(&mut s, Rook, White, 0, 0);
                put(&mut s, Rook, White, 1, 0);
                put(&mut s, Pawn, Black, 1, 1);
                put(&mut s, Pawn, Black, 1, 2);
            }
            "two-square-escape" => {
                put(&mut s, Rook, White, 0, 8);
                put(&mut s, Rook, White, 1, 8);
            }
            "interposition" => {
                put(&mut s, CrownPrince, Black, 0, 0);
                put(&mut s, Rook, White, 0, 8);
                put(&mut s, Rook, White, 1, 8);
                put(&mut s, Rook, Black, 4, 1);
            }
            "incoming-mate" => {
                put(&mut s, Rook, White, 2, 3);
                put(&mut s, Rook, White, 1, 8);
                put(&mut s, Pawn, Black, 2, 0);
                put(&mut s, Pawn, Black, 2, 2);
                s.set_current_turn(White);
            }
            _ => {
                for (x, y) in [(0, 1), (1, 0), (1, 1)] {
                    put(&mut s, Pawn, Black, x, y);
                }
            }
        }
        s.recompute_hash();
        s.reset_rep_history();
        xs.push((name.to_string(), s));
    }
    xs
}
fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err(
            "usage: royal_probe_smoke describe|micro|search|probe-stress CORPUS.json".into(),
        );
    }
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(&args[2]).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    let mut all = Vec::new();
    for c in cases {
        let (s, cp) = load(&c)?;
        all.push((c.name, s, cp));
    }
    let fixture_cp = all.first().ok_or("empty corpus")?.2.clone();
    for (name, s) in fixtures() {
        all.push((name, s, fixture_cp.clone()));
    }
    let variants = ["off", "A40", "A80", "A160", "Ablocked", "Lold", "Lverified"];
    let mut rng = rand::rngs::StdRng::seed_from_u64(20260915);
    match args[1].as_str() {
        "probe-stress" => {
            for piece in [PieceType::Tengu, PieceType::FreeEagle] {
                let mut s = GameState::new();
                s.clear_board();
                put(&mut s, piece, Color::Black, 18, 18);
                put(&mut s, PieceType::King, Color::Black, 35, 35);
                put(&mut s, PieceType::King, Color::White, 0, 0);
                let before = BoardPosition::from_state(&s);
                for rep in 0..5 {
                    let r = mate_probe(&s, Duration::from_millis(2), 64);
                    assert_eq!(BoardPosition::from_state(&s), before);
                    emit(serde_json::json!({"piece":format!("{piece:?}"),"rep":rep,"probe":r}));
                }
            }
        }
        "describe" => {
            for (name, s, cp) in &mut all {
                let before = BoardPosition::from_state(s);
                let w = weights(&cp.weights, "Lold");
                let old = term_scores(s.get_board(), &w).1;
                let verified = {
                    let _m = set_modes(false, true);
                    term_scores(s.get_board(), &w).1
                };
                let start = Instant::now();
                let evasions = diagnostic_evasions(s);
                let ev_us = start.elapsed().as_micros();
                let mut probe = Vec::new();
                for _ in 0..3 {
                    probe.push(mate_probe(s, Duration::from_millis(2), 64));
                }
                assert_eq!(BoardPosition::from_state(s), before);
                emit(
                    serde_json::json!({"case":name,"Lold":old,"Lverified":verified,"evasion_count":evasions.as_ref().map(|e|e.len()),"evasion_generation_us":ev_us,"reused_defense_penalty":evasions.as_ref().map(|e|defense_penalty(e)),"mate_probe":probe}),
                );
            }
        }
        "micro" => {
            for rep in 0..5 {
                let mut order: Vec<_> = (0..all.len()).collect();
                order.shuffle(&mut rng);
                for i in order {
                    let (name, s, cp) = &mut all[i];
                    let mut vs = variants;
                    vs.shuffle(&mut rng);
                    for v in vs {
                        let w = weights(&cp.weights, v);
                        let _m = set_modes(v == "Ablocked", v == "Lverified");
                        s.ensure_eval_inc(&w);
                        let mut n = 0u64;
                        let t = Instant::now();
                        while t.elapsed() < Duration::from_millis(25) {
                            for _ in 0..100 {
                                black_box(evaluate_with_ply(black_box(s), black_box(&w), 0));
                                n += 1;
                            }
                        }
                        emit(
                            serde_json::json!({"case":name,"rep":rep,"variant":v,"calls":n,"ns_per_eval":t.elapsed().as_nanos() as f64/n as f64,"score":evaluate_with_ply(s,&w,0)}),
                        );
                    }
                }
            }
        }
        "search" => {
            for rep in 0..2 {
                let mut order: Vec<_> = (0..all.len()).collect();
                order.shuffle(&mut rng);
                for i in order {
                    let (name, s, cp) = &all[i];
                    // Targeted forced positions only; terminal/quiet tiny boards are micro fixtures.
                    if [
                        "xray-trap",
                        "quiet-box",
                        "incoming-mate",
                        "two-square-escape",
                    ]
                    .contains(&name.as_str())
                    {
                        continue;
                    }
                    let mut vs = variants;
                    vs.shuffle(&mut rng);
                    for v in vs {
                        let mut cp = cp.clone();
                        cp.weights = weights(&cp.weights, v);
                        cp.search_defaults.q_own_large_only = true;
                        let player = AlphaBetaPlayer::from_checkpoint(cp);
                        let mut cfg = player.config().clone();
                        cfg.depth = 8;
                        cfg.max_time_ms = Some(3000);
                        cfg.collect_trace = false;
                        let _m = set_modes(v == "Ablocked", v == "Lverified");
                        let t = Instant::now();
                        let r = search(s, player.weights(), &cfg);
                        let ms = t.elapsed().as_secs_f64() * 1000.;
                        emit(
                            serde_json::json!({"case":name,"rep":rep,"variant":v,"nodes":r.nodes,"qnodes":r.q_nodes,"ms":ms,"depth":r.completed_depth,"score":r.score,"best":r.best_move,"root_lines":r.root_lines,"aborted":r.aborted}),
                        );
                    }
                }
            }
        }
        _ => return Err("unknown mode".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("royal_probe_smoke: {e}");
        std::process::exit(1);
    }
}
