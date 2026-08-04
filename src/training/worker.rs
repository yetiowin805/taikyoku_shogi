//! Game-playing worker: (agents, start, seed) → GameRecordV2.

use crate::board_position::BoardPosition;
use crate::game_history::GameResult;
use crate::game_state::GameState;
use crate::piece::Color;
use crate::player::{player_by_name_with_options, AgentOptions, Player};
use crate::training::record::{
    move_to_record, AgentSpec, GameRecordV2, GameStart, GameStats, FORMAT_VERSION,
};
use std::path::Path;
use std::time::Instant;

pub const DEFAULT_MAX_MOVES: usize = 20_000;

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub black: AgentSpec,
    pub white: AgentSpec,
    pub start: GameStart,
    pub seed: u64,
    pub max_moves: usize,
    /// When true, print move progress to stdout.
    pub verbose: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            black: AgentSpec::new("ab"),
            white: AgentSpec::new("ab"),
            start: GameStart::Opening,
            seed: 0,
            max_moves: DEFAULT_MAX_MOVES,
            verbose: false,
        }
    }
}

fn agent_options(spec: &AgentSpec) -> AgentOptions {
    AgentOptions {
        depth: spec.depth,
        model: spec.model.clone(),
        max_time_ms: spec.max_time_ms,
        quiescence_depth: spec.quiescence_depth,
    }
}

fn make_player(spec: &AgentSpec) -> Result<Box<dyn Player>, String> {
    player_by_name_with_options(&spec.name, &agent_options(spec))
}

fn initial_state(start: &GameStart) -> GameState {
    match start {
        GameStart::Opening => {
            let mut state = GameState::new();
            state.setup_initial_position();
            state
        }
        GameStart::Position { position } => position.to_state(),
    }
}

/// Play one game and return a complete record (not yet written to disk).
pub fn play_one_game(config: &WorkerConfig) -> Result<GameRecordV2, String> {
    let black = make_player(&config.black)?;
    let white = make_player(&config.white)?;
    let mut state = initial_state(&config.start);
    let started = Instant::now();
    let game_id = GameRecordV2::new_id(config.seed);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut moves = Vec::new();
    let mut move_number = 1usize;
    let mut result: Option<GameResult> = None;

    while move_number <= config.max_moves {
        if state.is_draw_by_500_move_rule() {
            result = Some(GameResult::Draw);
            break;
        }
        if state.is_draw_by_insufficient_material() {
            result = Some(GameResult::Draw);
            break;
        }
        if let Some(winner) = state.get_winner() {
            result = Some(match winner {
                Color::Black => GameResult::BlackWins,
                Color::White => GameResult::WhiteWins,
            });
            break;
        }

        let legal = state.generate_legal_moves();
        if legal.is_empty() {
            result = Some(match state.get_current_turn() {
                Color::Black => GameResult::WhiteWins,
                Color::White => GameResult::BlackWins,
            });
            break;
        }

        let color = state.get_current_turn();
        let player: &dyn Player = match color {
            Color::Black => black.as_ref(),
            Color::White => white.as_ref(),
        };
        let Some(mv) = player.choose_move(&state) else {
            return Err(format!(
                "Player {} returned no move with {} legal moves",
                player.name(),
                legal.len()
            ));
        };

        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv.clone());
        if state.get_current_turn() == turn_before {
            return Err(format!(
                "Move failed to change turn at move {} ({:?})",
                move_number, mv
            ));
        }

        moves.push(move_to_record(&mv, color, move_number));
        if config.verbose {
            println!(
                "{}. {:?}: {}{}-{}{}",
                move_number,
                color,
                mv.from.file,
                mv.from.rank,
                mv.to.file,
                mv.to.rank
            );
        }

        let step = if mv.is_two_step() { 2 } else { 1 };
        move_number += step;
    }

    if result.is_none() {
        result = Some(GameResult::Draw);
    }

    let move_count = moves.len();
    Ok(GameRecordV2 {
        format_version: FORMAT_VERSION,
        game_id,
        seed: config.seed,
        black: config.black.clone(),
        white: config.white.clone(),
        start: config.start.clone(),
        moves,
        result,
        stats: GameStats {
            move_count,
            elapsed_ms: Some(started.elapsed().as_millis() as u64),
        },
        timestamp,
    })
}

/// Replay recorded moves onto a fresh start position (for featurization).
pub fn replay_to_end(record: &GameRecordV2) -> Result<GameState, String> {
    let mut state = initial_state(&record.start);
    for mr in &record.moves {
        let mv = crate::game_history::GameHistory::record_to_move(mr)?;
        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv);
        if state.get_current_turn() == turn_before {
            return Err(format!(
                "Replay failed at move {} in game {}",
                mr.move_number, record.game_id
            ));
        }
    }
    Ok(state)
}

/// Snapshot after applying the first `ply` recorded moves (0 = start).
pub fn replay_to_ply(record: &GameRecordV2, ply: usize) -> Result<GameState, String> {
    let mut state = initial_state(&record.start);
    for mr in record.moves.iter().take(ply) {
        let mv = crate::game_history::GameHistory::record_to_move(mr)?;
        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv);
        if state.get_current_turn() == turn_before {
            return Err(format!(
                "Replay failed at move {} in game {}",
                mr.move_number, record.game_id
            ));
        }
    }
    Ok(state)
}

pub fn start_from_position(pos: BoardPosition) -> GameStart {
    GameStart::Position { position: pos }
}

#[derive(Debug, Default, Clone)]
pub struct BatchSummary {
    pub games: usize,
    pub black_wins: usize,
    pub white_wins: usize,
    pub draws: usize,
    pub total_moves: usize,
}

impl BatchSummary {
    pub fn record(&mut self, result: &Option<GameResult>, move_count: usize) {
        self.games += 1;
        self.total_moves += move_count;
        match result {
            Some(GameResult::BlackWins) => self.black_wins += 1,
            Some(GameResult::WhiteWins) => self.white_wins += 1,
            Some(GameResult::Draw) | None => self.draws += 1,
        }
    }

    pub fn mean_length(&self) -> f64 {
        if self.games == 0 {
            0.0
        } else {
            self.total_moves as f64 / self.games as f64
        }
    }

    pub fn print(&self) {
        println!(
            "Batch: {} games | B {} / W {} / D {} | mean length {:.1}",
            self.games,
            self.black_wins,
            self.white_wins,
            self.draws,
            self.mean_length()
        );
    }
}

/// Shared config for a finite parallel batch (used by `worker batch` and `worker daemon`).
#[derive(Debug, Clone)]
pub struct BatchConfig {
    pub games: usize,
    pub starts: Vec<GameStart>,
    pub outdir: String,
    pub seed_base: u64,
    pub black: AgentSpec,
    pub white: AgentSpec,
    pub jobs: usize,
    pub max_moves: usize,
    pub verbose: bool,
}

#[derive(Debug, Default, Clone)]
pub struct BatchOutcome {
    pub summary: BatchSummary,
    pub errors: Vec<String>,
    pub last_game_id: Option<String>,
    pub games_ok: usize,
    pub games_failed: usize,
}

/// Play `cfg.games` self-play games in parallel into `cfg.outdir`.
pub fn run_batch(cfg: &BatchConfig) -> Result<BatchOutcome, String> {
    if cfg.games == 0 {
        return Ok(BatchOutcome::default());
    }
    if cfg.starts.is_empty() {
        return Err("run_batch: no starts".into());
    }
    std::fs::create_dir_all(&cfg.outdir).map_err(|e| format!("outdir: {}", e))?;

    let jobs = cfg.jobs.max(1);
    let summary_mu = std::sync::Mutex::new(BatchSummary::default());
    let next = std::sync::Mutex::new(0usize);
    let errors = std::sync::Mutex::new(Vec::<String>::new());
    let last_id = std::sync::Mutex::new(None::<String>);
    let failed = std::sync::Mutex::new(0usize);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let summary_mu = &summary_mu;
            let next = &next;
            let errors = &errors;
            let last_id = &last_id;
            let failed = &failed;
            scope.spawn(move || loop {
                let g = {
                    let mut n = next.lock().unwrap();
                    if *n >= cfg.games {
                        return;
                    }
                    let g = *n;
                    *n += 1;
                    g
                };
                let start = cfg.starts[g % cfg.starts.len()].clone();
                let worker_cfg = WorkerConfig {
                    black: cfg.black.clone(),
                    white: cfg.white.clone(),
                    start,
                    seed: cfg.seed_base.wrapping_add(g as u64),
                    max_moves: cfg.max_moves,
                    verbose: cfg.verbose && jobs == 1,
                };
                match play_one_game(&worker_cfg) {
                    Ok(record) => {
                        let path = Path::new(&cfg.outdir).join(format!("{}.json", record.game_id));
                        if let Err(e) = record.save_path(&path) {
                            *failed.lock().unwrap() += 1;
                            errors.lock().unwrap().push(e);
                            continue;
                        }
                        summary_mu
                            .lock()
                            .unwrap()
                            .record(&record.result, record.stats.move_count);
                        *last_id.lock().unwrap() = Some(record.game_id.clone());
                        if cfg.verbose && jobs == 1 {
                            println!("Game {} -> {}", g + 1, path.display());
                        }
                    }
                    Err(e) => {
                        *failed.lock().unwrap() += 1;
                        errors.lock().unwrap().push(format!("game {}: {}", g, e));
                    }
                }
            });
        }
    });

    let summary = summary_mu.into_inner().unwrap();
    Ok(BatchOutcome {
        games_ok: summary.games,
        games_failed: failed.into_inner().unwrap(),
        last_game_id: last_id.into_inner().unwrap(),
        errors: errors.into_inner().unwrap(),
        summary,
    })
}
