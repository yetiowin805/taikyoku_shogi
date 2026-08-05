//! Start-position pool generation and per-game start sources.

use crate::board_position::BoardPosition;
use crate::player::{player_by_name_with_options, AgentOptions};
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::record::{AgentSpec, GameStart};
use crate::training::start_gen::{generate_fischer_start, is_recipe_path};
use crate::training::worker::{play_one_game, WorkerConfig};
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
    /// When false (default), emit Fischer-style openings. When true, play from
    /// the fixed opening until `until_move` (legacy midgame snapshots).
    pub from_play: bool,
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
            from_play: false,
        }
    }
}

/// Where batch/daemon games get their start positions.
#[derive(Debug, Clone)]
pub enum StartsSource {
    /// Cycle a fixed list (`opening` or loaded pool JSON).
    Fixed(Vec<GameStart>),
    /// Fresh Fischer-style opening (mirrored rank shuffle + ablations) per game.
    Fischer,
}

impl StartsSource {
    /// Start position for game index `g` with `seed` (typically `seed_base + g`).
    pub fn start_for_game(&self, g: usize, seed: u64) -> Result<GameStart, String> {
        match self {
            StartsSource::Fixed(starts) => {
                if starts.is_empty() {
                    return Err("run_batch: no starts".into());
                }
                Ok(starts[g % starts.len()].clone())
            }
            StartsSource::Fischer => {
                let (pos, _recipe) = generate_fischer_start(seed);
                Ok(GameStart::Position { position: pos })
            }
        }
    }
}

/// Parse worker `--starts` / `STARTS` value.
///
/// - `opening` — standard initial position
/// - `random` / `fischer` — per-game Fischer shuffle + powerful/royal ablations
/// - otherwise — directory of saved start JSON files
pub fn parse_starts_spec(spec: &str) -> Result<StartsSource, String> {
    if spec == "opening" {
        return Ok(StartsSource::Fixed(vec![GameStart::Opening]));
    }
    if spec == "random" || spec == "fischer" {
        return Ok(StartsSource::Fischer);
    }
    if spec.starts_with("random:") || spec.starts_with("fischer:") {
        return Err(format!(
            "Invalid starts '{}': use 'random' or 'fischer' (no plies). \
             Legacy midgame snapshots: pool generate --from-play",
            spec
        ));
    }
    let loaded = load_starts_dir(Path::new(spec))?;
    if loaded.is_empty() {
        return Err(format!("No starts in {}", spec));
    }
    Ok(StartsSource::Fixed(
        loaded
            .into_iter()
            .map(|(_, p)| GameStart::Position { position: p })
            .collect(),
    ))
}

/// Generate start positions (Fischer openings by default).
pub fn generate_pool(cfg: &PoolGenerateConfig) -> Result<Vec<String>, String> {
    ensure_data_dirs()?;
    std::fs::create_dir_all(&cfg.outdir)
        .map_err(|e| format!("Failed to create {}: {}", cfg.outdir, e))?;

    let mut ids = Vec::with_capacity(cfg.count);
    for i in 0..cfg.count {
        let seed = cfg.seed_base.wrapping_add(i as u64);
        let id = format!("{:016x}-{:04}", seed, i);
        let path = Path::new(&cfg.outdir).join(format!("{}.json", id));
        if cfg.from_play {
            let pos = generate_one_play_start(cfg, seed)?;
            pos.save_path(&path)?;
        } else {
            let (pos, recipe) = generate_fischer_start(seed);
            pos.save_path(&path)?;
            let recipe_path = Path::new(&cfg.outdir).join(format!("{}.recipe.json", id));
            recipe.save_path(&recipe_path)?;
        }
        ids.push(id);
    }
    Ok(ids)
}

/// Play from opening with `cfg.agent` / noise until `cfg.until_move`, return snapshot.
pub fn generate_one_play_start(
    cfg: &PoolGenerateConfig,
    seed: u64,
) -> Result<BoardPosition, String> {
    use crate::game_state::GameState;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

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

/// Load all starts from a directory (or empty if missing). Skips `*.recipe.json`.
pub fn load_starts_dir(dir: &Path) -> Result<Vec<(String, BoardPosition)>, String> {
    let mut out = Vec::new();
    for path in paths::list_json_files(dir)? {
        if is_recipe_path(&path) {
            continue;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_starts_fischer_aliases() {
        assert!(matches!(
            parse_starts_spec("random").unwrap(),
            StartsSource::Fischer
        ));
        assert!(matches!(
            parse_starts_spec("fischer").unwrap(),
            StartsSource::Fischer
        ));
        assert!(matches!(
            parse_starts_spec("opening").unwrap(),
            StartsSource::Fixed(_)
        ));
        assert!(parse_starts_spec("random:300").is_err());
    }

    #[test]
    fn fischer_start_varies_by_seed() {
        let src = parse_starts_spec("random").unwrap();
        let a = src.start_for_game(0, 1).unwrap();
        let b = src.start_for_game(1, 2).unwrap();
        match (a, b) {
            (GameStart::Position { position: pa }, GameStart::Position { position: pb }) => {
                let ja = serde_json::to_string(&pa).unwrap();
                let jb = serde_json::to_string(&pb).unwrap();
                assert_ne!(ja, jb);
            }
            _ => panic!("expected Position starts"),
        }
    }
}
