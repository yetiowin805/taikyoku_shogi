//! Timed A/B of same-wipe sibling-search experiments vs PathAware baseline.
//!
//!   cargo run --release --bin q_blowup_exp
//!   cargo run --release --bin q_blowup_exp -- --time-ms=1000 --depth=4
//!   cargo run --release --bin q_blowup_exp -- --depth=2 --time-ms=20000

use std::path::Path;
use std::time::Instant;
use taikyoku_shogi::board_position::BoardPosition;
use taikyoku_shogi::game_state::{GameState, Move};
use taikyoku_shogi::notation::move_encode;
use taikyoku_shogi::piece::{Color, PieceType};
use taikyoku_shogi::position::Position;
use taikyoku_shogi::eval::{evaluate_with_ply, EvalWeights};
use taikyoku_shogi::path_utils;
use taikyoku_shogi::search::{
    generate_loud_promotions, is_loud_promotion_move, probe_quiescence, search, QPruneMode,
    SearchConfig, SearchResult,
};
use taikyoku_shogi::training::record::load_game_json;
use taikyoku_shogi::training::worker::replay_to_ply;

struct Variant {
    name: &'static str,
    sibling_mode: u8,
    q_no_pathclear_after_wipe: bool,
    q_loud_promo_simple_only: bool,
}

const VARIANTS: &[Variant] = &[
    Variant {
        name: "baseline",
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        q_loud_promo_simple_only: false,
    },
    Variant {
        name: "loud-st",
        sibling_mode: 0,
        q_no_pathclear_after_wipe: false,
        q_loud_promo_simple_only: true,
    },
    Variant {
        name: "lmr2+st",
        sibling_mode: 2,
        q_no_pathclear_after_wipe: false,
        q_loud_promo_simple_only: true,
    },
];

fn opening() -> GameState {
    let mut state = GameState::new();
    state.setup_initial_position();
    state
}

fn pawn_midgame() -> GameState {
    let mut state = GameState::new();
    state.setup_initial_position();
    for file in [4u8, 8, 12, 16, 20, 24, 28] {
        for &(from_rank, to_rank, color) in &[
            (10u8, 11u8, Color::Black),
            (25u8, 24u8, Color::White),
        ] {
            state.set_current_turn(color);
            let from = Position::new(file, from_rank).unwrap();
            let to = Position::new(file, to_rank).unwrap();
            if state.get_board().get_piece(from).map(|p| p.piece_type) == Some(PieceType::Pawn)
                && state.get_board().get_piece(to).is_none()
            {
                let _ = state.make_move_for_search(Move::new(from, to));
            }
        }
    }
    state.set_current_turn(Color::Black);
    state
}

fn after_gg() -> GameState {
    let mut state = opening();
    let from = Position::new(17, 3).unwrap();
    let to = Position::new(17, 25).unwrap();
    let mv = state
        .generate_legal_moves()
        .into_iter()
        .find(|m| m.from == from && m.to == to)
        .expect("GG 18,4→18,26");
    let _ = state.make_move_for_search(mv);
    state
}

fn load_start(path: &str) -> GameState {
    BoardPosition::load_path(Path::new(path))
        .unwrap_or_else(|e| panic!("load {path}: {e}"))
        .to_state()
}

fn replay_game(path: &str, ply: usize) -> GameState {
    let rec = load_game_json(&std::fs::read_to_string(path).unwrap()).unwrap();
    replay_to_ply(&rec, ply).unwrap_or_else(|e| panic!("replay {path} ply {ply}: {e}"))
}

fn cfg(v: &Variant, depth: u32, time_ms: u64) -> SearchConfig {
    SearchConfig {
        depth,
        max_time_ms: Some(time_ms),
        collect_trace: false,
        quiescence_depth: 2,
        q_prune_mode: QPruneMode::PathAware,
        track_q_unique: true,
        sibling_mode: v.sibling_mode,
        q_no_pathclear_after_wipe: v.q_no_pathclear_after_wipe,
        q_loud_promo_simple_only: v.q_loud_promo_simple_only,
        ..Default::default()
    }
}

fn best_label(r: &SearchResult) -> String {
    r.best_move
        .as_ref()
        .map(move_encode)
        .unwrap_or_else(|| "-".into())
}

fn run_one(name: &str, state: &GameState, weights: &EvalWeights, depth: u32, time_ms: u64) {
    let legal = state.generate_legal_moves().len();
    println!(
        "\n=== {name} legal={legal} turn={:?} depth={depth} time={time_ms}ms ===",
        state.get_current_turn()
    );
    println!(
        "{:<10} {:>7} {:>10} {:>8} {:>10} {:>6} {:>6} {:>5} {:>8} {:>+8} {}",
        "var", "ms", "nodes", "nps", "qnodes", "qPC%", "scored", "abort", "score", "dscore", "best"
    );
    let mut base_score: Option<i32> = None;
    let mut base_best: Option<String> = None;
    for v in VARIANTS {
        let t0 = Instant::now();
        let r = search(state, weights, &cfg(v, depth, time_ms));
        let ms = t0.elapsed().as_millis().max(1) as u64;
        let nps = r.nodes.saturating_mul(1000) / ms;
        let qpc = if r.q_caps_searched == 0 {
            0
        } else {
            r.q_kind_path.saturating_mul(100) / r.q_caps_searched
        };
        let best = best_label(&r);
        let dscore = base_score.map(|b| r.score - b).unwrap_or(0);
        let move_mark = match &base_best {
            None => "",
            Some(b) if *b == best => "",
            Some(_) => " MOVE",
        };
        println!(
            "{:<10} {:>7} {:>10} {:>8} {:>10} {:>6} {:>6} {:>5} {:>8} {:>+8} {}{}",
            v.name,
            ms,
            r.nodes,
            nps,
            r.q_nodes,
            qpc,
            r.root_moves_scored,
            if r.aborted { "Y" } else { "" },
            r.score,
            dscore,
            best,
            move_mark
        );
        if base_score.is_none() {
            base_score = Some(r.score);
            base_best = Some(best);
        }
    }
}

fn sq_label(state: &GameState, file: u8, rank: u8) -> String {
    let pos = Position::new(file, rank).unwrap();
    match state.get_board().get_piece(pos) {
        None => format!("({file},{rank}) empty"),
        Some(p) => format!(
            "({file},{rank}) {:?} {:?}{}",
            p.color,
            p.piece_type,
            if p.is_promoted { "+" } else { "" }
        ),
    }
}

fn path_victims(state: &GameState, mv: &Move) -> Vec<String> {
    let us = state.get_current_turn();
    path_utils::get_path_positions(mv.from, mv.to)
        .into_iter()
        .filter_map(|pos| {
            let p = state.get_board().get_piece(pos)?;
            Some(format!(
                "{},{} {:?} {:?}{}",
                pos.file,
                pos.rank,
                p.color,
                p.piece_type,
                if p.color == us { " (own)" } else { "" }
            ))
        })
        .collect()
}

fn explain_miss(weights: &EvalWeights) {
    let state = load_start("data/raw/starts/0000000000000009-0002.json");
    println!(
        "start-0009 turn={:?} legal={} ply≈{}",
        state.get_current_turn(),
        state.generate_legal_moves().len(),
        state.get_move_history().len()
    );
    println!("  {}", sq_label(&state, 29, 22));
    println!("  {}", sq_label(&state, 29, 0));
    println!("  {}", sq_label(&state, 35, 13));
    println!("  {}", sq_label(&state, 22, 0));

    let keep = state
        .generate_legal_moves()
        .into_iter()
        .find(|m| m.from.file == 29 && m.from.rank == 22 && m.to.file == 29 && m.to.rank == 0)
        .expect("29,22-29,0");
    let miss = state
        .generate_legal_moves()
        .into_iter()
        .find(|m| m.from.file == 35 && m.from.rank == 13 && m.to.file == 22 && m.to.rank == 0)
        .expect("35,13-22,0");

    for (name, mv) in [("keep 29,22-29,0", &keep), ("alt  35,13-22,0", &miss)] {
        println!(
            "\n{name} {} promo={} victims:",
            move_encode(mv),
            mv.promoted
        );
        for v in path_victims(&state, mv) {
            println!("    {v}");
        }
        let mut child = state.clone();
        let _ = child.make_move_for_search(mv.clone());
        let static_s = evaluate_with_ply(&child, weights, child.get_move_history().len());
        let q = probe_quiescence(&child, weights, 2, QPruneMode::PathAware, None);
        println!(
            "    after: stm={:?} static(child)={static_s} q2={}/{} qPC={}",
            child.get_current_turn(),
            q.score,
            q.q_nodes,
            if q.q_caps_searched == 0 {
                0
            } else {
                q.q_kind_path.saturating_mul(100) / q.q_caps_searched
            }
        );
        let promos = generate_loud_promotions(&child);
        let mut pc = 0u32;
        let mut simple = 0u32;
        for pm in &promos {
            let between = path_utils::get_path_positions(pm.from, pm.to)
                .iter()
                .any(|p| *p != pm.to && child.get_board().get_piece(*p).is_some());
            if between {
                pc += 1;
            } else {
                simple += 1;
            }
            if pc + simple <= 12 || between {
                let from_p = child.get_board().get_piece(pm.from);
                println!(
                    "    loud {} {:?} pathclear={between}",
                    move_encode(pm),
                    from_p.map(|p| p.piece_type)
                );
            }
        }
        println!(
            "    loud promos: {} (pathclear≈{pc} simple≈{simple}) is_loud_promo_move={}",
            promos.len(),
            is_loud_promotion_move(&state, mv)
        );
    }

    println!("\n--- d2 root scores for the two moves ---");
    for v in VARIANTS {
        let r = search(&state, weights, &cfg(v, 2, 20000));
        let mut lines: Vec<(String, i32)> = r
            .root_lines
            .iter()
            .map(|(m, s)| (move_encode(m), *s))
            .collect();
        lines.sort_by(|a, b| b.1.cmp(&a.1));
        let find = |tok: &str| {
            lines
                .iter()
                .find(|(l, _)| l.starts_with(tok))
                .map(|(l, s)| format!("{l}={s}"))
                .unwrap_or_else(|| format!("{tok}=?"))
        };
        println!(
            "  {:<10} best={} score={}  {}  {}",
            v.name,
            best_label(&r),
            r.score,
            find("29,22-29,0"),
            find("35,13-22,0")
        );
        print!("    top5:");
        for (l, s) in lines.iter().take(5) {
            print!(" {l}={s}");
        }
        println!();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let depth: u32 = args
        .iter()
        .find_map(|a| a.strip_prefix("--depth=")?.parse().ok())
        .unwrap_or(4);
    let time_ms: u64 = args
        .iter()
        .find_map(|a| a.strip_prefix("--time-ms=")?.parse().ok())
        .unwrap_or(1000);
    let only = args.iter().find_map(|a| a.strip_prefix("--only="));
    let weights = EvalWeights::seed();
    if args.iter().any(|a| a == "--explain-miss") {
        explain_miss(&weights);
        return;
    }
    println!("q_blowup_exp depth={depth} time_ms={time_ms} sibling A/B");

    let mut jobs: Vec<(&str, GameState)> = vec![
        ("opening", opening()),
        ("pawn-mid", pawn_midgame()),
        ("after-GG", after_gg()),
        (
            "start-0009",
            load_start("data/raw/starts/0000000000000009-0002.json"),
        ),
        (
            "vg-blowup",
            load_start("data/raw/starts/vg-file6-blowup.json"),
        ),
        ("vg-ply30", replay_game("games/game_1786914877.json", 30)),
        (
            "loud-p80",
            replay_game(
                "data/raw/games/interesting-loud-swiss/slot0012-T50C50-vs-T100C50-a-black.json",
                80,
            ),
        ),
        (
            "loud-p400",
            replay_game(
                "data/raw/games/interesting-loud-swiss/slot0012-T50C50-vs-T100C50-a-black.json",
                400,
            ),
        ),
    ];
    if let Some(o) = only {
        jobs.retain(|(n, _)| *n == o);
    }
    for (name, state) in &jobs {
        run_one(name, state, &weights, depth, time_ms);
    }
}
