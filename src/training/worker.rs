//! Game-playing worker: (agents, start, seed) → GameRecordV2.

use crate::board_position::BoardPosition;
use crate::game_history::GameResult;
use crate::game_state::GameState;
use crate::piece::Color;
use crate::player::{player_by_name_with_options, AgentOptions, Player};
use crate::training::pool::StartsSource;
use crate::training::record::{
    move_to_record_with_eval, AgentSpec, GameRecordV2, GameStart, GameStats, FORMAT_VERSION,
};
use rand::rngs::OsRng;
use rand::RngCore;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    /// When set and true, abort the game between moves.
    pub stop: Option<Arc<AtomicBool>>,
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
            stop: None,
        }
    }
}

/// Mid-game abort: includes moves played so far for inspection.
#[derive(Debug, Clone)]
pub struct PlayFailure {
    pub message: String,
    pub partial: GameRecordV2,
}

impl std::fmt::Display for PlayFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

fn agent_options(spec: &AgentSpec) -> AgentOptions {
    AgentOptions {
        depth: spec.depth,
        model: spec.model.clone(),
        max_time_ms: spec.max_time_ms,
        quiescence_depth: spec.quiescence_depth,
        engine: None,
    }
}

fn make_player(spec: &AgentSpec) -> Result<Box<dyn Player>, String> {
    if let Some(engine) = spec.engine.as_deref() {
        return Ok(Box::new(crate::external_player::ExternalAbPlayer::spawn(
            engine, spec,
        )?));
    }
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

fn build_record(
    config: &WorkerConfig,
    game_id: String,
    timestamp: u64,
    started: Instant,
    moves: Vec<crate::game_history::MoveRecord>,
    result: Option<GameResult>,
    abort_reason: Option<String>,
) -> GameRecordV2 {
    let move_count = moves.len();
    GameRecordV2 {
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
        abort_reason,
    }
}

/// Play one game and return a complete record (not yet written to disk).
///
/// On mid-game failure, returns [`PlayFailure`] with a partial record (moves so far).
pub fn play_one_game(config: &WorkerConfig) -> Result<GameRecordV2, PlayFailure> {
    let black = make_player(&config.black).map_err(|message| PlayFailure {
        message,
        partial: build_record(
            config,
            GameRecordV2::new_id(config.seed),
            0,
            Instant::now(),
            Vec::new(),
            None,
            Some("failed before any moves".into()),
        ),
    })?;
    let white = make_player(&config.white).map_err(|message| PlayFailure {
        message,
        partial: build_record(
            config,
            GameRecordV2::new_id(config.seed),
            0,
            Instant::now(),
            Vec::new(),
            None,
            Some("failed before any moves".into()),
        ),
    })?;
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

    let abort = |message: String, moves: Vec<_>| -> PlayFailure {
        let partial = build_record(
            config,
            game_id.clone(),
            timestamp,
            started,
            moves,
            None,
            Some(message.clone()),
        );
        PlayFailure { message, partial }
    };

    while move_number <= config.max_moves {
        if config
            .stop
            .as_ref()
            .is_some_and(|s| s.load(Ordering::Relaxed))
        {
            return Err(abort("stopped".into(), moves));
        }
        if state.is_draw_by_500_move_rule() {
            result = Some(GameResult::Draw);
            break;
        }
        if state.is_draw_by_fivefold_repetition() {
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
        let Some((mv, ann)) = player.choose_move_annotated(&state) else {
            return Err(abort(
                format!(
                    "Player {} returned no move with {} legal moves",
                    player.name(),
                    legal.len()
                ),
                moves,
            ));
        };

        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv.clone());
        if state.get_current_turn() == turn_before {
            return Err(abort(
                format!(
                    "Move failed to change turn at move {} ({:?})",
                    move_number, mv
                ),
                moves,
            ));
        }

        moves.push(move_to_record_with_eval(
            &mv,
            color,
            move_number,
            ann.eval,
            ann.static_eval,
            ann.nodes,
        ));
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

    Ok(build_record(
        config,
        game_id,
        timestamp,
        started,
        moves,
        result,
        None,
    ))
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
    pub starts: StartsSource,
    pub outdir: String,
    /// When `0`, each game draws a fresh seed from OS entropy; otherwise `seed_base + g`.
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
    pub partials_saved: usize,
}

/// Resolve the RNG seed for game index `g`.
/// `seed_base == 0` → fresh OS entropy; otherwise `seed_base + g` (deterministic).
pub fn game_seed(seed_base: u64, g: usize) -> u64 {
    if seed_base == 0 {
        OsRng.next_u64()
    } else {
        seed_base.wrapping_add(g as u64)
    }
}

/// Play `cfg.games` self-play games in parallel into `cfg.outdir`.
///
/// Failed mid-game runs write a partial [`GameRecordV2`] under `{outdir}/partial/`.
pub fn run_batch(cfg: &BatchConfig) -> Result<BatchOutcome, String> {
    if cfg.games == 0 {
        return Ok(BatchOutcome::default());
    }
    if matches!(&cfg.starts, StartsSource::Fixed(s) if s.is_empty()) {
        return Err("run_batch: no starts".into());
    }
    std::fs::create_dir_all(&cfg.outdir).map_err(|e| format!("outdir: {}", e))?;
    let partial_dir = Path::new(&cfg.outdir).join("partial");
    std::fs::create_dir_all(&partial_dir).map_err(|e| format!("partial dir: {}", e))?;

    let jobs = cfg.jobs.max(1);
    let summary_mu = std::sync::Mutex::new(BatchSummary::default());
    let next = std::sync::Mutex::new(0usize);
    let errors = std::sync::Mutex::new(Vec::<String>::new());
    let last_id = std::sync::Mutex::new(None::<String>);
    let failed = std::sync::Mutex::new(0usize);
    let partials = std::sync::Mutex::new(0usize);

    std::thread::scope(|scope| {
        for _ in 0..jobs {
            let summary_mu = &summary_mu;
            let next = &next;
            let errors = &errors;
            let last_id = &last_id;
            let failed = &failed;
            let partials = &partials;
            let partial_dir = &partial_dir;
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
                let seed = game_seed(cfg.seed_base, g);
                let start = match cfg.starts.start_for_game(g, seed) {
                    Ok(s) => s,
                    Err(e) => {
                        errors.lock().unwrap().push(e);
                        *failed.lock().unwrap() += 1;
                        continue;
                    }
                };
                let worker_cfg = WorkerConfig {
                    black: cfg.black.clone(),
                    white: cfg.white.clone(),
                    start,
                    seed,
                    max_moves: cfg.max_moves,
                    verbose: cfg.verbose && jobs == 1,
                    stop: None,
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
                    Err(fail) => {
                        *failed.lock().unwrap() += 1;
                        let path = partial_dir.join(format!("{}.json", fail.partial.game_id));
                        match fail.partial.save_path(&path) {
                            Ok(()) => {
                                *partials.lock().unwrap() += 1;
                                errors.lock().unwrap().push(format!(
                                    "game {}: {} (partial -> {})",
                                    g,
                                    fail.message,
                                    path.display()
                                ));
                            }
                            Err(e) => {
                                errors.lock().unwrap().push(format!(
                                    "game {}: {}; also failed to save partial: {}",
                                    g, fail.message, e
                                ));
                            }
                        }
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
        partials_saved: partials.into_inner().unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::game_seed;

    #[test]
    fn deterministic_seed_base_plus_index() {
        assert_eq!(game_seed(100, 0), 100);
        assert_eq!(game_seed(100, 7), 107);
    }

    #[test]
    fn zero_seed_base_draws_os_entropy() {
        let samples: Vec<u64> = (0..8).map(|g| game_seed(0, g)).collect();
        // Not the old deterministic 0..7 sequence.
        assert_ne!(samples, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        // Distinct draws (collision across 8 OS samples is negligible).
        let mut uniq = samples.clone();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), samples.len());
    }
}
