//! Opt-in search measurements. Never part of the correctness test suite.
use serde::Deserialize;
use serde_json::json;
use std::{path::PathBuf, time::Instant};
use taikyoku_shogi::{
    alphabeta_player::AlphaBetaPlayer,
    piece::Color,
    player::AgentOptions,
    search::{is_loud_promotion_move, move_captures_enemy, search},
    training::{record::load_game_json, worker::replay_to_ply},
};
#[derive(Deserialize)]
struct Case {
    name: String,
    group: String,
    game: PathBuf,
    ply: usize,
    model: PathBuf,
}
fn configure(variant: &str) {
    #[cfg(feature = "search-experiments")]
    {
        use taikyoku_shogi::optimization::*;
        let mut opts = Options::default();
        // The randomized study varies only these three factors.
        let fields: Vec<_> = variant.split('-').collect();
        assert_eq!(fields.len(), 3, "expected S0-W0-T0 configuration");
        let swap: u8 = fields[0].strip_prefix('S').unwrap().parse().unwrap();
        let width: i32 = fields[1].strip_prefix('W').unwrap().parse().unwrap();
        let tt: u8 = fields[2].strip_prefix('T').unwrap().parse().unwrap();
        assert!(swap <= 1 && [0, 100, 500, 2000].contains(&width) && tt <= 2);
        opts.swap_remove = swap == 1;
        opts.aspiration = width;
        opts.tt_first = tt;
        configure(opts);
    }
    #[cfg(not(feature = "search-experiments"))]
    {
        assert!(variant == "stock" || variant == "production");
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        args.len() >= 6,
        "CORPUS INDEX DEPTH TIME_MS VARIANT [REPEATS]"
    );
    let path = PathBuf::from(&args[1]);
    let cases: Vec<Case> = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let i: usize = args[2].parse().unwrap();
    let depth: u32 = args[3].parse().unwrap();
    let budget: u64 = args[4].parse().unwrap();
    let variant = &args[5];
    configure(variant);
    let c = &cases[i];
    let base = path.parent().unwrap();
    let resolve = |p: &PathBuf| {
        if p.is_absolute() {
            p.clone()
        } else {
            base.join(p)
        }
    };
    let rec = load_game_json(&std::fs::read_to_string(resolve(&c.game)).unwrap()).unwrap();
    assert!(c.ply <= rec.moves.len());
    let t = Instant::now();
    let state = replay_to_ply(&rec, c.ply).unwrap();
    let replay_ms = t.elapsed().as_secs_f64() * 1000.;
    let agent = if state.get_current_turn() == Color::Black {
        &rec.black
    } else {
        &rec.white
    };
    let opts = AgentOptions {
        depth: Some(depth.max(1)),
        model: Some(resolve(&c.model).to_string_lossy().into()),
        max_time_ms: if budget == 0 { None } else { Some(budget) },
        quiescence_depth: agent.quiescence_depth,
    };
    let player = AlphaBetaPlayer::from_options(&opts);
    let board = state.get_board();
    let mut royals = vec![];
    let mut checks = vec![];
    let mut pieces = 0;
    for color in [Color::Black, Color::White] {
        let army = board.pieces_by_color(color);
        pieces += army.len();
        let r: Vec<_> = army.iter().filter(|p| p.piece_type.is_royal()).collect();
        checks.push(
            r.len() == 1
                && board.is_position_attacked_by_color_for_check(r[0].position, color.opposite()),
        );
        royals.push(r.len());
    }
    let legal = state.generate_legal_moves();
    let meta = json!({"index":i,"name":c.name,"group":c.group,"applied_plies":c.ply,"turn":state.get_current_turn(),
        "history_len":state.get_move_history().len(),"repetition_count":state.repetition_count(),"position_hash":state.hash(),
        "model":c.model,"settings":format!("{:?}",player.config()),"pieces":pieces,"royals":royals,"checks":checks,
        "winner":state.get_winner(),"legal":legal.len(),"captures":legal.iter().filter(|m|move_captures_enemy(&state,m)).count(),
        "multi_leg_captures":legal.iter().filter(|m|(m.is_two_step()||m.is_free_eagle())&&move_captures_enemy(&state,m)).count(),
        "promotions":legal.iter().filter(|m|is_loud_promotion_move(&state,m)).count(),"replay_ms":replay_ms});
    if depth == 0 {
        println!("{meta}");
        return;
    }
    let repeats: usize = args.get(6).map(|v| v.parse().unwrap()).unwrap_or(1);
    let mut warm_ms = None;
    if args.get(7).is_some_and(|s| s == "previous") && c.ply >= 2 {
        let previous = replay_to_ply(&rec, c.ply - 2).unwrap();
        let started = Instant::now();
        let _ = search(&previous, player.weights(), player.config());
        warm_ms = Some(started.elapsed().as_secs_f64() * 1000.);
    }
    for repeat in 0..repeats {
        let started = Instant::now();
        let r = search(&state, player.weights(), player.config());
        let ms = started.elapsed().as_secs_f64() * 1000.;
        let lines: Vec<_> = r.root_lines.iter().map(|(m, s)| json!([m, s])).collect();
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        let mut row = json!({"meta":meta,"variant":variant,"in_process_repeat":repeat,"depth":depth,"budget_ms":budget,
            "ms":ms,"completed_depth":r.completed_depth,"nodes":r.nodes,"qnodes":r.q_nodes,"score":r.score,
            "warm_previous_ms":warm_ms,"static_eval":r.static_eval,"best":r.best_move,"root_lines":lines,"aborted":r.aborted,
            "peak_rss_kib":usage.ru_maxrss});
        #[cfg(feature = "search-experiments")]
        {
            row["counters"] = json!(taikyoku_shogi::optimization::counters());
        }
        println!("{row}");
    }
}
