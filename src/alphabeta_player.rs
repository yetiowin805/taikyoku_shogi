//! Alpha-beta player wired to eval checkpoints.

use crate::eval::{load_checkpoint_or_seed, EvalCheckpoint, EvalWeights, DEFAULT_MODEL_PATH};
use crate::game_state::{GameState, Move};
use crate::player::AgentOptions;
use crate::search::{search, SearchConfig};
use std::env;
use std::path::{Path, PathBuf};

pub struct AlphaBetaPlayer {
    weights: EvalWeights,
    config: SearchConfig,
}

impl AlphaBetaPlayer {
    pub fn new(weights: EvalWeights, config: SearchConfig) -> Self {
        Self { weights, config }
    }

    /// Built-in seed weights, default search depth.
    pub fn seed() -> Self {
        Self::new(EvalWeights::seed(), SearchConfig::default())
    }

    /// Load from `models/ab-seed.json` if present, else built-in seed.
    /// Honors `TAIKYOKU_AB_MODEL` and `TAIKYOKU_AB_DEPTH` / `TAIKYOKU_AB_TIME_MS`.
    pub fn from_env_or_default() -> Self {
        Self::from_options(&AgentOptions::default())
    }

    /// Build from explicit options (GUI/API), falling back to env then checkpoint defaults.
    pub fn from_options(opts: &AgentOptions) -> Self {
        let model_path = opts
            .model
            .as_ref()
            .map(PathBuf::from)
            .or_else(|| env::var("TAIKYOKU_AB_MODEL").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_MODEL_PATH));

        let checkpoint = load_checkpoint_or_seed(&model_path);
        Self::from_checkpoint_with_overrides(checkpoint, opts)
    }

    pub fn from_checkpoint(checkpoint: EvalCheckpoint) -> Self {
        Self::from_checkpoint_with_overrides(checkpoint, &AgentOptions::default())
    }

    fn from_checkpoint_with_overrides(checkpoint: EvalCheckpoint, opts: &AgentOptions) -> Self {
        let mut config = SearchConfig {
            depth: checkpoint.search_defaults.depth.max(1),
            max_time_ms: checkpoint.search_defaults.max_time_ms,
        };
        if let Ok(d) = env::var("TAIKYOKU_AB_DEPTH") {
            if let Ok(v) = d.parse::<u32>() {
                config.depth = v.max(1);
            }
        }
        if let Ok(t) = env::var("TAIKYOKU_AB_TIME_MS") {
            config.max_time_ms = t.parse::<u64>().ok();
        }
        // Explicit options win (GUI / API).
        if let Some(d) = opts.depth {
            config.depth = d.max(1);
        }
        if let Some(t) = opts.max_time_ms {
            config.max_time_ms = Some(t);
        }
        Self::new(checkpoint.weights, config)
    }

    pub fn with_depth(mut self, depth: u32) -> Self {
        self.config.depth = depth.max(1);
        self
    }

    pub fn with_model_path(path: impl AsRef<Path>) -> Self {
        let checkpoint = load_checkpoint_or_seed(path.as_ref());
        Self::from_checkpoint(checkpoint)
    }

    pub fn make_move(game_state: &GameState) -> Option<Move> {
        Self::from_env_or_default().choose_move_inner(game_state)
    }

    pub fn choose_move_inner(&self, game_state: &GameState) -> Option<Move> {
        self.analyze(game_state).best_move
    }

    /// Full search with eval / tree trace for the GUI.
    pub fn analyze(&self, game_state: &GameState) -> crate::search::SearchResult {
        search(game_state, &self.weights, &self.config)
    }

    pub fn search_info(&self, game_state: &GameState) -> crate::search::SearchInfo {
        let side = match game_state.get_current_turn() {
            crate::piece::Color::Black => "Black",
            crate::piece::Color::White => "White",
        };
        let result = self.analyze(game_state);
        crate::search::search_info_from_result("ab", side, self.config.depth, &result)
    }

    pub fn weights(&self) -> &EvalWeights {
        &self.weights
    }

    pub fn config(&self) -> &SearchConfig {
        &self.config
    }
}
