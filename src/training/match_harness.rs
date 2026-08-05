//! Paired match harness: same start, colors reversed.

use crate::board_position::BoardPosition;
use crate::game_history::GameResult;
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::pool::load_starts_dir;
use crate::training::record::{AgentSpec, GameStart};
use crate::training::worker::{play_one_game, BatchSummary, WorkerConfig};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MatchConfig {
    pub engine_a: AgentSpec,
    pub engine_b: AgentSpec,
    pub starts_dir: String,
    pub outdir: String,
    pub seed_base: u64,
    pub max_games: usize,
    pub max_moves: usize,
    pub verbose: bool,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            engine_a: AgentSpec::new("ab"),
            engine_b: AgentSpec::new("ab"),
            starts_dir: paths::RAW_STARTS.to_string(),
            outdir: paths::RAW_GAMES.to_string(),
            seed_base: 1,
            max_games: 10,
            max_moves: crate::training::worker::DEFAULT_MAX_MOVES,
            verbose: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MatchScoreboard {
    pub pairs: usize,
    pub a_wins: usize,
    pub b_wins: usize,
    pub draws: usize,
}

impl MatchScoreboard {
    pub fn print(&self) {
        println!(
            "Match: {} pairs | A {} / B {} / D {} | score A {:.1} — B {:.1}",
            self.pairs,
            self.a_wins,
            self.b_wins,
            self.draws,
            self.a_wins as f64 + 0.5 * self.draws as f64,
            self.b_wins as f64 + 0.5 * self.draws as f64,
        );
    }
}

fn result_for_a(black_is_a: bool, result: &Option<GameResult>) -> MatchPoint {
    match result {
        Some(GameResult::Draw) | None => MatchPoint::Draw,
        Some(GameResult::BlackWins) => {
            if black_is_a {
                MatchPoint::A
            } else {
                MatchPoint::B
            }
        }
        Some(GameResult::WhiteWins) => {
            if black_is_a {
                MatchPoint::B
            } else {
                MatchPoint::A
            }
        }
    }
}

enum MatchPoint {
    A,
    B,
    Draw,
}

/// Play paired games from pool starts (A as Black then A as White on same start).
pub fn run_matches(cfg: &MatchConfig) -> Result<MatchScoreboard, String> {
    ensure_data_dirs()?;
    std::fs::create_dir_all(&cfg.outdir)
        .map_err(|e| format!("create {}: {}", cfg.outdir, e))?;

    let starts = load_starts_dir(Path::new(&cfg.starts_dir))?;
    let starts: Vec<(String, BoardPosition)> = if starts.is_empty() {
        // Fall back to opening if no pool yet.
        vec![("opening".into(), {
            let mut s = crate::game_state::GameState::new();
            s.setup_initial_position();
            BoardPosition::from_state(&s)
        })]
    } else {
        starts
    };

    let mut board = MatchScoreboard::default();
    let mut summary = BatchSummary::default();
    let mut game_idx = 0usize;

    for (start_id, pos) in starts.iter() {
        if board.pairs >= cfg.max_games {
            break;
        }
        let start = GameStart::Position {
            position: pos.clone(),
        };

        // Game 1: A black, B white
        let seed1 = cfg.seed_base.wrapping_add(game_idx as u64);
        let rec1 = play_one_game(&WorkerConfig {
            black: cfg.engine_a.clone(),
            white: cfg.engine_b.clone(),
            start: start.clone(),
            seed: seed1,
            max_moves: cfg.max_moves,
            verbose: cfg.verbose,
        })
        .map_err(|e| e.message)?;
        let path1 = Path::new(&cfg.outdir).join(format!("{}-a-black.json", rec1.game_id));
        rec1.save_path(&path1)?;
        summary.record(&rec1.result, rec1.stats.move_count);
        match result_for_a(true, &rec1.result) {
            MatchPoint::A => board.a_wins += 1,
            MatchPoint::B => board.b_wins += 1,
            MatchPoint::Draw => board.draws += 1,
        }
        game_idx += 1;

        // Game 2: B black, A white (colors reversed)
        let seed2 = cfg.seed_base.wrapping_add(game_idx as u64);
        let rec2 = play_one_game(&WorkerConfig {
            black: cfg.engine_b.clone(),
            white: cfg.engine_a.clone(),
            start,
            seed: seed2,
            max_moves: cfg.max_moves,
            verbose: cfg.verbose,
        })
        .map_err(|e| e.message)?;
        let path2 = Path::new(&cfg.outdir).join(format!("{}-a-white.json", rec2.game_id));
        rec2.save_path(&path2)?;
        summary.record(&rec2.result, rec2.stats.move_count);
        match result_for_a(false, &rec2.result) {
            MatchPoint::A => board.a_wins += 1,
            MatchPoint::B => board.b_wins += 1,
            MatchPoint::Draw => board.draws += 1,
        }
        game_idx += 1;

        board.pairs += 1;
        if cfg.verbose {
            println!("Completed pair on start {}", start_id);
        }
    }

    summary.print();
    board.print();
    Ok(board)
}
