//! Offline featurizer: raw games → labeled position rows under data/derived.
//!
//! Default sampling is event-driven: capture-burst ends + quiet stride until a
//! stable piece-count lead, then a few late samples, then even subsample to a
//! per-game target (default 150).

use crate::eval::{EvalWeights, ALL_PIECE_TYPES};
use crate::game_history::{GameHistory, GameResult};
use crate::game_state::{GameState, Move};
use crate::piece::Color;
use crate::training::paths::{self, ensure_data_dirs};
use crate::training::record::{GameRecordV2, GameStart};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const FEATURE_FORMAT_VERSION: u32 = 1;

/// Default quiet stride while the game is still contested.
pub const DEFAULT_QUIET_STRIDE: usize = 24;
/// Default even subsample size per game (`0` = keep all candidates).
pub const DEFAULT_TARGET_PER_GAME: usize = 150;
/// Piece-count ratio for "decided" (leader / trailer).
pub const DEFAULT_DECIDED_RATIO: f32 = 1.5;
/// Minimum absolute piece-count gap for decided.
pub const DEFAULT_DECIDED_MIN_GAP: i32 = 10;
/// Hysteresis ratio that must hold until game end.
pub const DEFAULT_DECIDED_HOLD: f32 = 1.25;
/// Spaced samples after the decided ply (before subsample).
pub const DEFAULT_POST_DECIDED_SAMPLES: usize = 3;

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
    /// Absolute black-perspective material eval under seed weights (diagnostic).
    pub seed_eval: f32,
}

#[derive(Debug, Clone)]
pub struct FeaturizeConfig {
    /// Even subsample size per game after building the candidate set (`0` = all).
    pub target_per_game: usize,
    /// Quiet-phase stride (plies between samples when not at a capture burst).
    pub quiet_stride: usize,
    pub decided_ratio: f32,
    pub decided_min_gap: i32,
    pub decided_hold: f32,
    pub post_decided_samples: usize,
    pub games_dir: String,
    pub out_dir: String,
}

impl Default for FeaturizeConfig {
    fn default() -> Self {
        Self {
            target_per_game: DEFAULT_TARGET_PER_GAME,
            quiet_stride: DEFAULT_QUIET_STRIDE,
            decided_ratio: DEFAULT_DECIDED_RATIO,
            decided_min_gap: DEFAULT_DECIDED_MIN_GAP,
            decided_hold: DEFAULT_DECIDED_HOLD,
            post_decided_samples: DEFAULT_POST_DECIDED_SAMPLES,
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

fn army_counts(state: &GameState) -> (i32, i32) {
    (
        state.get_board().get_pieces_by_color(Color::Black).len() as i32,
        state.get_board().get_pieces_by_color(Color::White).len() as i32,
    )
}

/// True if `mv` removed any enemy material (dest or path clear).
pub fn move_was_capture(state_before: &GameState, mv: &Move) -> bool {
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

fn lead(b: i32, w: i32) -> (f32, i32, Color) {
    if b >= w {
        let r = if w <= 0 {
            f32::INFINITY
        } else {
            b as f32 / w as f32
        };
        (r, b - w, Color::Black)
    } else {
        let r = if b <= 0 {
            f32::INFINITY
        } else {
            w as f32 / b as f32
        };
        (r, w - b, Color::White)
    }
}

fn is_blowout(b: i32, w: i32, ratio: f32, min_gap: i32) -> bool {
    let (r, gap, _) = lead(b, w);
    r >= ratio && gap >= min_gap
}

/// Earliest ply where lead meets threshold and never falls below hold afterward.
pub fn find_decided_ply(
    counts: &[(i32, i32)],
    ratio: f32,
    min_gap: i32,
    hold: f32,
) -> Option<usize> {
    let n = counts.len();
    for t in 0..n {
        if !is_blowout(counts[t].0, counts[t].1, ratio, min_gap) {
            continue;
        }
        let (_, _, leader) = lead(counts[t].0, counts[t].1);
        let ok = (t..n).all(|u| {
            let (r, gap, now) = lead(counts[u].0, counts[u].1);
            now == leader && r >= hold && gap >= (min_gap / 2).max(1)
        });
        if ok {
            return Some(t);
        }
    }
    None
}

/// Evenly pick up to `target` indices from a sorted candidate list.
pub fn evenly_subsample(cands: &[usize], target: usize) -> Vec<usize> {
    if cands.is_empty() || target == 0 {
        return cands.to_vec();
    }
    if cands.len() <= target {
        return cands.to_vec();
    }
    let mut out = Vec::with_capacity(target);
    for k in 0..target {
        let idx = if target == 1 {
            0
        } else {
            k * (cands.len() - 1) / (target - 1)
        };
        out.push(cands[idx]);
    }
    out.dedup();
    if out.len() < target {
        for &c in cands {
            if !out.contains(&c) {
                out.push(c);
                if out.len() >= target {
                    break;
                }
            }
        }
        out.sort_unstable();
    }
    out
}

/// Build candidate ply indices for one game (before subsample).
pub fn sample_candidate_plies(
    was_capture_into: &[bool],
    counts: &[(i32, i32)],
    cfg: &FeaturizeConfig,
) -> (Vec<usize>, Option<usize>) {
    let n = was_capture_into.len().saturating_sub(1);
    debug_assert_eq!(counts.len(), n + 1);
    let decided = find_decided_ply(
        counts,
        cfg.decided_ratio,
        cfg.decided_min_gap,
        cfg.decided_hold,
    );
    let contested_end = decided.unwrap_or(n);
    let mut sampled = vec![false; n + 1];
    sampled[0] = true;

    for i in 1..=contested_end {
        if was_capture_into[i] && (i == n || !was_capture_into[i + 1]) {
            sampled[i] = true;
        }
    }

    let mut last_quiet = 0usize;
    for i in 1..=contested_end {
        if was_capture_into[i] {
            continue;
        }
        if i - last_quiet >= cfg.quiet_stride {
            sampled[i] = true;
            last_quiet = i;
        }
    }

    if let Some(t) = decided {
        if t < n && cfg.post_decided_samples > 0 {
            let span = n - t;
            let kmax = cfg.post_decided_samples;
            for k in 0..kmax {
                let i = (t + (span * (k + 1)) / (kmax + 1)).min(n);
                sampled[i] = true;
            }
        }
    }

    let cands: Vec<usize> = sampled
        .iter()
        .enumerate()
        .filter(|(_, s)| **s)
        .map(|(i, _)| i)
        .collect();
    (cands, decided)
}

pub fn featurize_game(
    record: &GameRecordV2,
    cfg: &FeaturizeConfig,
    weights: &EvalWeights,
) -> Result<Vec<LabeledPosition>, String> {
    let label = result_to_label(&record.result);
    let n = record.moves.len();
    let mut state = initial_state(&record.start);
    let mut was_capture_into = vec![false; n + 1];
    let mut counts = Vec::with_capacity(n + 1);
    let mut snapshots: Vec<(Color, Vec<f32>, f32)> = Vec::with_capacity(n + 1);

    counts.push(army_counts(&state));
    snapshots.push((
        state.get_current_turn(),
        piece_diff_features(&state),
        crate::eval::evaluate_absolute_black(state.get_board(), weights, 0) as f32,
    ));

    for (i, mr) in record.moves.iter().enumerate() {
        let mv = GameHistory::record_to_move(mr)?;
        was_capture_into[i + 1] = move_was_capture(&state, &mv);
        let turn_before = state.get_current_turn();
        if state.make_move(mv).is_none() {
            return Err(format!(
                "featurize replay failed at move {} in {}",
                mr.move_number, record.game_id
            ));
        }
        if state.get_current_turn() == turn_before {
            return Err(format!(
                "featurize replay stuck at move {} in {}",
                mr.move_number, record.game_id
            ));
        }
        let ply = i + 1;
        counts.push(army_counts(&state));
        snapshots.push((
            state.get_current_turn(),
            piece_diff_features(&state),
            crate::eval::evaluate_absolute_black(state.get_board(), weights, ply) as f32,
        ));
    }

    let (cands, _decided) = sample_candidate_plies(&was_capture_into, &counts, cfg);
    let plies = evenly_subsample(&cands, cfg.target_per_game);

    let mut out = Vec::with_capacity(plies.len());
    for ply in plies {
        let (turn, piece_diff, seed_eval) = &snapshots[ply];
        out.push(LabeledPosition {
            format_version: FEATURE_FORMAT_VERSION,
            game_id: record.game_id.clone(),
            ply,
            result: label,
            turn: *turn,
            piece_diff: piece_diff.clone(),
            seed_eval: *seed_eval,
        });
    }
    Ok(out)
}

pub fn featurize_dir(cfg: &FeaturizeConfig) -> Result<usize, String> {
    ensure_data_dirs()?;
    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("Failed to create {}: {}", cfg.out_dir, e))?;
    for path in paths::list_json_files(Path::new(&cfg.out_dir))? {
        let _ = fs::remove_file(&path);
    }

    let weights = EvalWeights::seed();
    let mut total = 0usize;
    for path in paths::list_json_files(Path::new(&cfg.games_dir))? {
        let record = GameRecordV2::load_path(&path)?;
        let rows = featurize_game(&record, cfg, &weights)?;
        for (j, row) in rows.iter().enumerate() {
            let out_path = Path::new(&cfg.out_dir).join(format!("{}-{:04}.json", record.game_id, j));
            let json =
                serde_json::to_string(row).map_err(|e| format!("serialize feature: {}", e))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evenly_subsample_respects_target_and_endpoints() {
        let cands: Vec<usize> = (0..300).collect();
        let got = evenly_subsample(&cands, 150);
        assert_eq!(got.len(), 150);
        assert_eq!(got[0], 0);
        assert_eq!(*got.last().unwrap(), 299);
    }

    #[test]
    fn decided_ply_requires_stable_lead() {
        // Lead flips, ends even — never decided.
        let flip = vec![(40, 20), (20, 40), (30, 30)];
        assert!(find_decided_ply(&flip, 1.5, 10, 1.25).is_none());

        // Stable blowout from ply 2.
        let stable = vec![(30, 30), (32, 28), (40, 20), (42, 18), (45, 15)];
        assert_eq!(find_decided_ply(&stable, 1.5, 10, 1.25), Some(2));
        // Temporary spike then collapse.
        let spike = vec![(40, 20), (25, 25), (26, 24)];
        assert!(find_decided_ply(&spike, 1.5, 10, 1.25).is_none());
    }

    #[test]
    fn candidates_include_burst_ends_and_ply0() {
        // plies 0..=8; captures into 2,3 then quiet; capture into 6 then quiet.
        let was = vec![
            false, false, true, true, false, false, true, false, false,
        ];
        let counts = vec![(36, 36); was.len()];
        let cfg = FeaturizeConfig {
            quiet_stride: 100, // disable quiet stride noise
            target_per_game: 0,
            decided_ratio: 9.0, // no decided
            ..FeaturizeConfig::default()
        };
        let (cands, decided) = sample_candidate_plies(&was, &counts, &cfg);
        assert!(decided.is_none());
        assert!(cands.contains(&0));
        assert!(cands.contains(&3)); // end of first burst
        assert!(cands.contains(&6)); // end of second burst
        assert!(!cands.contains(&2)); // mid-burst
    }
}
