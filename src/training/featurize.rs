//! Offline featurizer: raw games → labeled position rows under data/derived.

use crate::eval::{EvalWeights, ALL_PIECE_TYPES};
use crate::game_history::GameResult;
use crate::game_state::GameState;
use crate::piece::Color;
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::record::GameRecordV2;
use crate::training::worker::replay_to_ply;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FEATURE_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledPosition {
    pub format_version: u32,
    pub game_id: String,
    pub ply: usize,
    /// 1.0 = black win, 0.0 = white win, 0.5 = draw (from game result).
    pub result: f32,
    /// Side to move at this ply.
    pub turn: Color,
    /// Piece-count features: for each PieceType, (black_count - white_count).
    pub piece_diff: Vec<f32>,
    /// Absolute black-perspective material eval under seed weights (frozen modifiers).
    pub seed_eval: f32,
}

#[derive(Debug, Clone)]
pub struct FeaturizeConfig {
    /// Sample every Nth ply (1 = all).
    pub stride: usize,
    /// Skip positions immediately after a capturing move (classic Texel quiet).
    pub quiet_only: bool,
    /// Max positions per game (0 = unlimited).
    pub max_per_game: usize,
    pub games_dir: String,
    pub out_dir: String,
}

impl Default for FeaturizeConfig {
    fn default() -> Self {
        Self {
            stride: 8,
            quiet_only: true,
            max_per_game: 64,
            games_dir: paths::RAW_GAMES.to_string(),
            out_dir: paths::DERIVED_POSITIONS.to_string(),
        }
    }
}

fn result_to_label(result: &Option<GameResult>) -> f32 {
    match result {
        Some(GameResult::BlackWins) => 1.0,
        Some(GameResult::WhiteWins) => 0.0,
        Some(GameResult::Draw) | None => 0.5,
    }
}

fn piece_diff_features(state: &GameState) -> Vec<f32> {
    let board = state.get_board();
    let mut counts = vec![0.0f32; ALL_PIECE_TYPES.len()];
    for (i, &pt) in ALL_PIECE_TYPES.iter().enumerate() {
        let mut diff = 0i32;
        for p in board.get_pieces_by_color(Color::Black) {
            if p.piece_type == pt {
                diff += 1;
            }
        }
        for p in board.get_pieces_by_color(Color::White) {
            if p.piece_type == pt {
                diff -= 1;
            }
        }
        counts[i] = diff as f32;
    }
    counts
}

/// True if `mv` removed any enemy material (dest or path clear).
fn move_was_capture(state_before: &GameState, mv: &crate::game_state::Move) -> bool {
    let board = state_before.get_board();
    let Some(mover) = board.get_piece(mv.from) else {
        return false;
    };
    if board
        .get_piece(mv.to)
        .map(|p| p.color != mover.color)
        .unwrap_or(false)
    {
        return true;
    }
    for pos in crate::path_utils::get_path_positions(mv.from, mv.to) {
        if pos == mv.from || pos == mv.to {
            continue;
        }
        if board.get_piece(pos).is_some() {
            return true;
        }
    }
    if let Some(path) = mv.free_eagle_path() {
        for w in path.windows(2) {
            for pos in crate::path_utils::get_path_positions(w[0], w[1]) {
                if pos == w[0] {
                    continue;
                }
                if let Some(p) = board.get_piece(pos) {
                    if p.color != mover.color {
                        return true;
                    }
                }
            }
            if board
                .get_piece(w[1])
                .map(|p| p.color != mover.color)
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

pub fn featurize_game(
    record: &GameRecordV2,
    cfg: &FeaturizeConfig,
    weights: &EvalWeights,
) -> Result<Vec<LabeledPosition>, String> {
    let label = result_to_label(&record.result);
    let mut out = Vec::new();
    let n = record.moves.len();
    let mut i = 0usize;
    while i <= n {
        if cfg.stride > 1 && i % cfg.stride != 0 {
            i += 1;
            continue;
        }
        // Quiet = not immediately after a capture (Texel-style).
        if cfg.quiet_only && i > 0 {
            let before = replay_to_ply(record, i - 1)?;
            let mv = crate::game_history::GameHistory::record_to_move(&record.moves[i - 1])?;
            if move_was_capture(&before, &mv) {
                i += 1;
                continue;
            }
        }
        let state = replay_to_ply(record, i)?;
        let piece_diff = piece_diff_features(&state);
        let seed_eval =
            crate::eval::evaluate_absolute_black(state.get_board(), weights, i) as f32;
        out.push(LabeledPosition {
            format_version: FEATURE_FORMAT_VERSION,
            game_id: record.game_id.clone(),
            ply: i,
            result: label,
            turn: state.get_current_turn(),
            piece_diff,
            seed_eval,
        });
        if cfg.max_per_game > 0 && out.len() >= cfg.max_per_game {
            break;
        }
        i += 1;
    }
    Ok(out)
}

pub fn featurize_dir(cfg: &FeaturizeConfig) -> Result<usize, String> {
    ensure_data_dirs()?;
    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("Failed to create {}: {}", cfg.out_dir, e))?;
    // Wipe previous derived rows for this out_dir (disposable).
    for path in paths::list_json_files(Path::new(&cfg.out_dir))? {
        let _ = fs::remove_file(&path);
    }

    let weights = EvalWeights::seed();
    let mut total = 0usize;
    for path in paths::list_json_files(Path::new(&cfg.games_dir))? {
        let record = GameRecordV2::load_path(&path)?;
        let rows = featurize_game(&record, cfg, &weights)?;
        for (j, row) in rows.iter().enumerate() {
            let out_path = Path::new(&cfg.out_dir).join(format!(
                "{}-{:04}.json",
                record.game_id, j
            ));
            let json = serde_json::to_string(row)
                .map_err(|e| format!("serialize feature: {}", e))?;
            fs::write(&out_path, json)
                .map_err(|e| format!("write {}: {}", out_path.display(), e))?;
            total += 1;
        }
    }
    Ok(total)
}

pub fn load_labeled_dir(dir: &Path) -> Result<Vec<LabeledPosition>, String> {
    let mut rows = Vec::new();
    for path in paths::list_json_files(dir)? {
        let contents = fs::read_to_string(&path)
            .map_err(|e| format!("read {}: {}", path.display(), e))?;
        let row: LabeledPosition = serde_json::from_str(&contents)
            .map_err(|e| format!("parse {}: {}", path.display(), e))?;
        rows.push(row);
    }
    Ok(rows)
}
