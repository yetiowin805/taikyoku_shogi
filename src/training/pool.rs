//! Start-position pool generation.

use crate::board_position::BoardPosition;
use crate::player::{player_by_name_with_options, AgentOptions};
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::record::AgentSpec;
use crate::training::worker::{play_one_game, WorkerConfig};
use crate::training::record::GameStart;
use rand::Rng;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct PoolGenerateConfig {
    pub agent: AgentSpec,
    pub until_move: usize,
    pub count: usize,
    pub seed_base: u64,
    pub outdir: String,
    /// Fraction of legal moves sampled uniformly instead of agent choice (0..1).
    pub noise: f64,
}

impl Default for PoolGenerateConfig {
    fn default() -> Self {
        Self {
            agent: AgentSpec::new("random"),
            until_move: 300,
            count: 10,
            seed_base: 1,
            outdir: paths::RAW_STARTS.to_string(),
            noise: 0.0,
        }
    }
}

/// Play from opening with optional noise until `until_move`, save BoardPositions.
pub fn generate_pool(cfg: &PoolGenerateConfig) -> Result<Vec<String>, String> {
    ensure_data_dirs()?;
    std::fs::create_dir_all(&cfg.outdir)
        .map_err(|e| format!("Failed to create {}: {}", cfg.outdir, e))?;

    let mut ids = Vec::with_capacity(cfg.count);
    for i in 0..cfg.count {
        let seed = cfg.seed_base.wrapping_add(i as u64);
        let pos = generate_one_start(cfg, seed)?;
        let id = format!("{:016x}-{:04}", seed, i);
        let path = Path::new(&cfg.outdir).join(format!("{}.json", id));
        pos.save_path(&path)?;
        ids.push(id);
    }
    Ok(ids)
}

fn generate_one_start(cfg: &PoolGenerateConfig, seed: u64) -> Result<BoardPosition, String> {
    use crate::game_state::GameState;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    let player = player_by_name_with_options(
        &cfg.agent.name,
        &AgentOptions {
            depth: cfg.agent.depth,
            model: cfg.agent.model.clone(),
            max_time_ms: cfg.agent.max_time_ms,
            quiescence_depth: cfg.agent.quiescence_depth,
        },
    )?;

    let mut rng = StdRng::seed_from_u64(seed);
    let mut state = GameState::new();
    state.setup_initial_position();

    let mut move_number = 1usize;
    while move_number <= cfg.until_move {
        if state.get_winner().is_some()
            || state.is_draw_by_500_move_rule()
            || state.is_draw_by_insufficient_material()
        {
            break;
        }
        let legal = state.generate_legal_moves();
        if legal.is_empty() {
            break;
        }

        let use_noise = cfg.noise > 0.0 && rng.gen::<f64>() < cfg.noise;
        let mv = if use_noise {
            legal[rng.gen_range(0..legal.len())].clone()
        } else if let Some(m) = player.choose_move(&state) {
            m
        } else {
            legal[rng.gen_range(0..legal.len())].clone()
        };

        let turn_before = state.get_current_turn();
        let _ = state.make_move(mv.clone());
        if state.get_current_turn() == turn_before {
            break;
        }
        let step = if mv.is_two_step() { 2 } else { 1 };
        move_number += step;
    }

    Ok(BoardPosition::from_state(&state))
}

/// Load all starts from a directory (or empty if missing).
pub fn load_starts_dir(dir: &Path) -> Result<Vec<(String, BoardPosition)>, String> {
    let mut out = Vec::new();
    for path in paths::list_json_files(dir)? {
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("start")
            .to_string();
        let pos = BoardPosition::load_path(&path)?;
        out.push((id, pos));
    }
    Ok(out)
}

/// Build a WorkerConfig start from opening or a loaded position.
pub fn game_start_opening() -> GameStart {
    GameStart::Opening
}

/// Convenience: play a short truncated game via worker and snapshot (no noise).
pub fn snapshot_via_worker(
    agent: &AgentSpec,
    until_move: usize,
    seed: u64,
) -> Result<BoardPosition, String> {
    let record = play_one_game(&WorkerConfig {
        black: agent.clone(),
        white: agent.clone(),
        start: GameStart::Opening,
        seed,
        max_moves: until_move,
        verbose: false,
    })
    .map_err(|e| e.message)?;
    let state = crate::training::worker::replay_to_end(&record)?;
    Ok(BoardPosition::from_state(&state))
}
