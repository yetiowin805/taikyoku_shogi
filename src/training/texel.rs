//! Texel-style logistic fit on derived piece-count features.

use crate::eval::{EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::piece::Color;
use crate::training::featurize::{load_labeled_dir, LabeledPosition};
use crate::training::paths;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TexelFitConfig {
    pub features_dir: String,
    pub out_model: String,
    pub iterations: usize,
    pub learning_rate: f32,
    /// Logistic scaling constant K (overridden by scale calibration when `fit_k`).
    pub k: f32,
    /// If true, set K so mean(|K · eval|) ≈ [`Self::k_target_abs`] (avoids the
    /// CE-minimizing collapse to K→0 that freezes gradients on self-play data).
    pub fit_k: bool,
    /// Target mean |K · material_eval| when calibrating K.
    pub k_target_abs: f32,
}

impl Default for TexelFitConfig {
    fn default() -> Self {
        Self {
            features_dir: paths::DERIVED_POSITIONS.to_string(),
            out_model: "models/ab-texel.json".to_string(),
            // Piece values are ~pawn units (Pawn≈1), not centipawns — old defaults
            // (k=1/400, lr=0.01, 50 iters) barely moved weights.
            iterations: 300,
            learning_rate: 0.5,
            k: 0.1,
            fit_k: true,
            k_target_abs: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TexelFitStats {
    pub k: f32,
    pub loss_before: f64,
    pub loss_after: f64,
    pub max_abs_delta: f32,
    pub mean_abs_delta: f32,
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

fn eval_from_diff(weights: &[f32], diff: &[f32]) -> f32 {
    let n = weights.len().min(diff.len());
    let mut s = 0.0f32;
    for i in 0..n {
        s += weights[i] * diff[i];
    }
    s
}

fn mean_abs_eval(rows: &[LabeledPosition], weights: &[f32]) -> f32 {
    if rows.is_empty() {
        return 1.0;
    }
    let mut sum = 0.0f32;
    for row in rows {
        sum += eval_from_diff(weights, &row.piece_diff).abs();
    }
    (sum / rows.len() as f32).max(1e-3)
}

/// Choose K so typical |K·e| is about `target` (order-1 logistic argument).
fn calibrate_k(rows: &[LabeledPosition], weights: &[f32], target: f32) -> f32 {
    let mean_abs = mean_abs_eval(rows, weights);
    (target / mean_abs).clamp(0.01, 2.0)
}

fn mean_cross_entropy(rows: &[LabeledPosition], weights: &[f32], k: f32) -> f64 {
    if rows.is_empty() {
        return 0.0;
    }
    let mut loss = 0.0f64;
    for row in rows {
        let e = eval_from_diff(weights, &row.piece_diff) as f64;
        let p = sigmoid(k as f64 * e).clamp(1e-6, 1.0 - 1e-6);
        let y = row.result as f64;
        loss += -(y * p.ln() + (1.0 - y) * (1.0 - p).ln());
    }
    loss / rows.len() as f64
}

/// Fit piece values via logistic regression (gradient descent).
///
/// All piece material weights are trainable (including royals and high-value
/// capturers). Rank PST / development / `royal_bonus_by_count` stay at seed:
/// featurize only emits piece-count diffs. Terminal 0-royals is not learned
/// here — [`EvalWeights::mate_score`] / `get_winner` already treat that as ±∞.
pub fn fit_texel(cfg: &TexelFitConfig) -> Result<(EvalCheckpoint, TexelFitStats), String> {
    let rows = load_labeled_dir(Path::new(&cfg.features_dir))?;
    if rows.is_empty() {
        return Err(format!(
            "No labeled positions in {}. Run featurize first.",
            cfg.features_dir
        ));
    }
    fit_texel_on_rows(cfg, &rows)
}

/// Same as [`fit_texel`] but with an in-memory dataset (tests / callers).
pub fn fit_texel_on_rows(
    cfg: &TexelFitConfig,
    rows: &[LabeledPosition],
) -> Result<(EvalCheckpoint, TexelFitStats), String> {
    if rows.is_empty() {
        return Err("No labeled positions".into());
    }

    let mut seed = EvalWeights::seed();
    let w0: Vec<f32> = ALL_PIECE_TYPES
        .iter()
        .map(|pt| seed.piece_value(*pt))
        .collect();
    let mut w = w0.clone();

    let mut k = if cfg.fit_k {
        calibrate_k(rows, &w, cfg.k_target_abs)
    } else {
        cfg.k
    };

    let loss_before = mean_cross_entropy(rows, &w, k);
    let lr = cfg.learning_rate;

    for iter in 0..cfg.iterations {
        // Re-scale K as weights move so the logistic stays in a useful regime.
        if cfg.fit_k && iter > 0 && iter % 25 == 0 {
            k = calibrate_k(rows, &w, cfg.k_target_abs);
        }

        let mut grad = vec![0.0f32; w.len()];
        for row in rows {
            let e = eval_from_diff(&w, &row.piece_diff);
            let p = sigmoid(k as f64 * e as f64) as f32;
            let err = p - row.result;
            let n = w.len().min(row.piece_diff.len());
            for i in 0..n {
                grad[i] += err * k * row.piece_diff[i];
            }
        }
        let inv_n = 1.0 / rows.len() as f32;
        for i in 0..w.len() {
            w[i] -= lr * grad[i] * inv_n;
            // Keep values positive-ish.
            if w[i] < 0.05 {
                w[i] = 0.05;
            }
        }
    }

    let mut max_abs_delta = 0.0f32;
    let mut sum_abs_delta = 0.0f32;
    for i in 0..w.len() {
        let d = (w[i] - w0[i]).abs();
        max_abs_delta = max_abs_delta.max(d);
        sum_abs_delta += d;
    }
    let mean_abs_delta = sum_abs_delta / w.len() as f32;

    for (i, &pt) in ALL_PIECE_TYPES.iter().enumerate() {
        seed.piece.insert(pt, w[i]);
    }
    seed.rebuild_piece_value_table();

    let loss_after = mean_cross_entropy(rows, &w, k);

    let stats = TexelFitStats {
        k,
        loss_before,
        loss_after,
        max_abs_delta,
        mean_abs_delta,
    };

    let mut cp = EvalCheckpoint::seed("ab-texel");
    cp.name = format!("ab-texel-k{:.6}", k);
    cp.weights = seed;
    if let Some(parent) = Path::new(&cfg.out_model).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    cp.save_path(&cfg.out_model)
        .map_err(|e| format!("save: {}", e))?;
    Ok((cp, stats))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::piece::PieceType;
    use crate::training::featurize::FEATURE_FORMAT_VERSION;

    fn pawn_idx() -> usize {
        ALL_PIECE_TYPES
            .iter()
            .position(|p| *p == PieceType::Pawn)
            .expect("Pawn in ALL_PIECE_TYPES")
    }

    fn synthetic_rows(n: usize) -> Vec<LabeledPosition> {
        let pi = pawn_idx();
        let mut rows = Vec::with_capacity(n);
        for i in 0..n {
            // Black pawn surplus → Black win; White surplus → White win.
            let black_ahead = i % 2 == 0;
            let mut diff = vec![0.0f32; ALL_PIECE_TYPES.len()];
            diff[pi] = if black_ahead { 5.0 } else { -5.0 };
            rows.push(LabeledPosition {
                format_version: FEATURE_FORMAT_VERSION,
                game_id: format!("syn-{i}"),
                ply: 10,
                result: if black_ahead { 1.0 } else { 0.0 },
                turn: Color::Black,
                piece_diff: diff,
                seed_eval: 0.0,
            });
        }
        rows
    }

    #[test]
    fn calibrate_k_targets_unit_scale() {
        let rows = synthetic_rows(20);
        let w: Vec<f32> = ALL_PIECE_TYPES
            .iter()
            .map(|pt| EvalWeights::seed().piece_value(*pt))
            .collect();
        let k = calibrate_k(&rows, &w, 1.0);
        let mean_abs = mean_abs_eval(&rows, &w);
        let mean_arg = k * mean_abs;
        assert!(
            (mean_arg - 1.0).abs() < 0.05,
            "expected |K·e|≈1, got {mean_arg} (k={k}, mean_abs={mean_abs})"
        );
    }

    #[test]
    fn fit_moves_weights_when_label_correlates_with_material() {
        let dir = std::env::temp_dir().join(format!(
            "taikyoku-texel-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let rows = synthetic_rows(64);
        let cfg = TexelFitConfig {
            features_dir: dir.to_string_lossy().into(),
            out_model: dir.join("out.json").to_string_lossy().into(),
            iterations: 120,
            learning_rate: 0.5,
            k: 0.1,
            fit_k: true,
            k_target_abs: 1.0,
        };
        let seed_pawn = EvalWeights::seed().piece_value(PieceType::Pawn);
        let (cp, stats) = fit_texel_on_rows(&cfg, &rows).expect("fit");
        assert!(
            stats.max_abs_delta > 0.05,
            "expected weights to move, max_abs_delta={}",
            stats.max_abs_delta
        );
        // Synthetic data: more pawns ⇒ win. Pawn value should rise.
        let fitted_pawn = cp.weights.piece_value(PieceType::Pawn);
        assert!(
            fitted_pawn > seed_pawn + 0.05,
            "pawn {seed_pawn} → {fitted_pawn} (maxΔ={})",
            stats.max_abs_delta
        );
        assert!(
            stats.loss_after <= stats.loss_before,
            "loss rose: before={} after={}",
            stats.loss_before,
            stats.loss_after
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
