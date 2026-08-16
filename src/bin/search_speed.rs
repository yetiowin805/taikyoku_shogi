//! Time-to-depth and 1s nps for opening + pawn-push midgame.
//!
//!   cargo run --release --bin search_speed
//!   cargo run --release --bin search_speed -- --time-ms=1000 --depth=8

use std::time::Instant;
use taikyoku_shogi::eval::EvalWeights;
use taikyoku_shogi::game_state::{GameState, Move};
use taikyoku_shogi::piece::{Color, PieceType};
use taikyoku_shogi::position::Position;
use taikyoku_shogi::search::{search, QPruneMode, SearchConfig};

fn opening() -> GameState {
    let mut state = GameState::new();
    state.setup_initial_position();
    state
}

fn midgame() -> GameState {
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

fn run(name: &str, state: &GameState, weights: &EvalWeights, depth: u32, time_ms: Option<u64>) {
    let cfg = SearchConfig {
        depth,
        max_time_ms: time_ms,
        collect_trace: false,
        quiescence_depth: 2,
        q_prune_mode: QPruneMode::PathAware,
        ..Default::default()
    };
    #[cfg(feature = "search-profile")]
    taikyoku_shogi::profile_timers::reset();
    let t0 = Instant::now();
    let r = search(state, weights, &cfg);
    let ms = t0.elapsed().as_millis();
    #[cfg(feature = "search-profile")]
    {
        let p = taikyoku_shogi::profile_timers::report();
        let tot = (ms as u128).saturating_mul(1_000_000);
        let pct = |ns: u128| {
            if tot == 0 {
                0
            } else {
                ns.saturating_mul(100) / tot
            }
        };
        let accounted = p.eval_ns + p.gen_ns + p.atk_ns + p.make_ns + p.order_ns;
        println!(
            "  profile eval={}% gen={}% (two_step={} std={} fe={} filter={}) attack={}% make={}% order={}% other≈{}%",
            pct(p.eval_ns),
            pct(p.gen_ns),
            pct(p.two_step_ns),
            pct(p.standard_gen_ns),
            pct(p.fe_gen_ns),
            pct(p.filter_ns),
            pct(p.atk_ns),
            pct(p.make_ns),
            pct(p.order_ns),
            100u128.saturating_sub(pct(accounted)),
        );
    }
    let nps = if ms > 0 {
        r.nodes.saturating_mul(1000) / ms as u64
    } else {
        0
    };
    let best = r.best_move.as_ref().map(|m| {
        format!(
            "{},{}→{},{}",
            m.from.file + 1,
            m.from.rank + 1,
            m.to.file + 1,
            m.to.rank + 1
        )
    });
    println!(
        "{name} depth={depth} time={time_ms:?} nodes={} nps={nps} q={} score={} best={:?} ms={ms}",
        r.nodes, r.q_nodes, r.score, best
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let depth: u32 = args
        .iter()
        .find_map(|a| a.strip_prefix("--depth=")?.parse().ok())
        .unwrap_or(3);
    let time_ms: Option<u64> = match args.iter().find_map(|a| a.strip_prefix("--time-ms=")) {
        None => None,
        Some("none") => None,
        Some(s) => s.parse().ok(),
    };
    let only = args.iter().find_map(|a| a.strip_prefix("--only="));
    let weights = EvalWeights::seed();
    println!("search_speed: depth={depth} time_ms={time_ms:?}");
    if only.is_none_or(|o| o == "opening") {
        run("opening", &opening(), &weights, depth, time_ms);
    }
    if only.is_none_or(|o| o == "midgame") {
        run("midgame", &midgame(), &weights, depth, time_ms);
    }
}
