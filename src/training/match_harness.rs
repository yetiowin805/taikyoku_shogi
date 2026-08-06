//! Paired match harness: same start, colors reversed.

use crate::game_history::GameResult;
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::pool::{parse_starts_spec, StartsSource};
use crate::training::record::{AgentSpec, GameStart};
use crate::training::worker::{play_one_game, BatchSummary, WorkerConfig};
use rand::rngs::OsRng;
use rand::RngCore;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct MatchConfig {
    pub engine_a: AgentSpec,
    pub engine_b: AgentSpec,
    /// Same as worker: `opening` | `random` | `light` | DIR.
    pub starts_spec: String,
    pub outdir: String,
    pub seed_base: u64,
    pub max_games: usize,
    pub max_moves: usize,
    pub jobs: usize,
    pub verbose: bool,
    pub stop: Option<Arc<AtomicBool>>,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            engine_a: AgentSpec::new("ab"),
            engine_b: AgentSpec::new("ab"),
            starts_spec: paths::RAW_STARTS.to_string(),
            outdir: paths::RAW_GAMES.to_string(),
            seed_base: 1,
            max_games: 10,
            max_moves: crate::training::worker::DEFAULT_MAX_MOVES,
            jobs: 1,
            verbose: false,
            stop: None,
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

fn resolve_start(starts: &StartsSource, pair_idx: usize, seed: u64) -> Result<GameStart, String> {
    starts.start_for_game(pair_idx, seed)
}

/// Play paired games (A as Black then A as White on same start).
pub fn run_matches(cfg: &MatchConfig) -> Result<MatchScoreboard, String> {
    ensure_data_dirs()?;
    std::fs::create_dir_all(&cfg.outdir)
        .map_err(|e| format!("create {}: {}", cfg.outdir, e))?;

    let starts = parse_starts_spec(&cfg.starts_spec).or_else(|e| {
        // Legacy: treat as opening fallback when path missing.
        if cfg.starts_spec == "opening" {
            Ok(StartsSource::Fixed(vec![GameStart::Opening]))
        } else {
            Err(e)
        }
    })?;

    let jobs = cfg.jobs.max(1);
    let board = Mutex::new(MatchScoreboard::default());
    let summary = Mutex::new(BatchSummary::default());
    let next_pair = AtomicUsize::new(0);
    let stop = cfg
        .stop
        .clone()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let starts = &starts;
            let board = &board;
            let summary = &summary;
            let next_pair = &next_pair;
            let stop = &stop;
            scope.spawn(move || loop {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let pair_idx = {
                    let n = next_pair.fetch_add(1, Ordering::Relaxed);
                    if n >= cfg.max_games {
                        return;
                    }
                    n
                };
                let start_seed = if cfg.seed_base == 0 {
                    let mut b = [0u8; 8];
                    OsRng.fill_bytes(&mut b);
                    u64::from_le_bytes(b)
                } else {
                    cfg.seed_base.wrapping_add(pair_idx as u64 * 2)
                };
                let start = match resolve_start(starts, pair_idx, start_seed) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("match start error: {e}");
                        return;
                    }
                };

                let play = |black: &AgentSpec, white: &AgentSpec, seed: u64, a_black: bool| {
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                    match play_one_game(&WorkerConfig {
                        black: black.clone(),
                        white: white.clone(),
                        start: start.clone(),
                        seed,
                        max_moves: cfg.max_moves,
                        verbose: cfg.verbose && jobs == 1,
                        stop: Some(stop.clone()),
                    }) {
                        Ok(rec) => {
                            let tag = if a_black { "a-black" } else { "a-white" };
                            let path =
                                Path::new(&cfg.outdir).join(format!("{}-{}.json", rec.game_id, tag));
                            let _ = rec.save_path(&path);
                            summary.lock().unwrap().record(&rec.result, rec.stats.move_count);
                            let mut b = board.lock().unwrap();
                            match result_for_a(a_black, &rec.result) {
                                MatchPoint::A => b.a_wins += 1,
                                MatchPoint::B => b.b_wins += 1,
                                MatchPoint::Draw => b.draws += 1,
                            }
                        }
                        Err(e) => {
                            if e.message != "stopped" {
                                eprintln!("match game failed: {}", e.message);
                            }
                        }
                    }
                };

                play(&cfg.engine_a, &cfg.engine_b, start_seed, true);
                play(
                    &cfg.engine_b,
                    &cfg.engine_a,
                    start_seed.wrapping_add(1),
                    false,
                );
                board.lock().unwrap().pairs += 1;
                if cfg.verbose {
                    println!("Completed pair {pair_idx}");
                }
            });
        }
    });

    let summary = summary.into_inner().unwrap();
    let board = board.into_inner().unwrap();
    summary.print();
    board.print();
    Ok(board)
}
