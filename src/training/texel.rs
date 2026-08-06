//! Texel-style logistic fit skeleton on derived features.

use crate::eval::{EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::training::featurize::{load_labeled_dir, LabeledPosition};
use crate::training::paths;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct TexelFitConfig {
    pub features_dir: String,
    pub out_model: String,
    pub iterations: usize,
    pub learning_rate: f32,
    /// Logistic scaling constant K (fit coarsely if `fit_k`).
    pub k: f32,
    pub fit_k: bool,
}

impl Default for TexelFitConfig {
    fn default() -> Self {
        Self {
            features_dir: paths::DERIVED_POSITIONS.to_string(),
            out_model: "models/ab-texel.json".to_string(),
            iterations: 50,
            learning_rate: 0.01,
            k: 1.0 / 400.0,
            fit_k: true,
        }
    }
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
pub fn fit_texel(cfg: &TexelFitConfig) -> Result<(EvalCheckpoint, f64), String> {
    let rows = load_labeled_dir(Path::new(&cfg.features_dir))?;
    if rows.is_empty() {
        return Err(format!(
            "No labeled positions in {}. Run featurize first.",
            cfg.features_dir
        ));
    }

    let mut seed = EvalWeights::seed();
    let mut w: Vec<f32> = ALL_PIECE_TYPES
        .iter()
        .map(|pt| seed.piece_value(*pt))
        .collect();

    let mut k = cfg.k;
    if cfg.fit_k {
        // Coarse 1D search for K.
        let mut best_k = k;
        let mut best_loss = mean_cross_entropy(&rows, &w, best_k);
        for scale in [0.25f32, 0.5, 1.0, 2.0, 4.0] {
            let candidate = cfg.k * scale;
            let loss = mean_cross_entropy(&rows, &w, candidate);
            if loss < best_loss {
                best_loss = loss;
                best_k = candidate;
            }
        }
        k = best_k;
    }

    let lr = cfg.learning_rate;
    for _ in 0..cfg.iterations {
        let mut grad = vec![0.0f32; w.len()];
        for row in &rows {
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

    for (i, &pt) in ALL_PIECE_TYPES.iter().enumerate() {
        seed.piece.insert(pt, w[i]);
    }
    seed.rebuild_piece_value_table();

    let loss = mean_cross_entropy(
        &rows,
        &ALL_PIECE_TYPES
            .iter()
            .map(|pt| seed.piece_value(*pt))
            .collect::<Vec<_>>(),
        k,
    );

    let mut cp = EvalCheckpoint::seed("ab-texel");
    cp.name = format!("ab-texel-k{:.6}", k);
    cp.weights = seed;
    if let Some(parent) = Path::new(&cfg.out_model).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {}", parent.display(), e))?;
    }
    cp.save_path(&cfg.out_model)
        .map_err(|e| format!("save: {}", e))?;
    Ok((cp, loss))
}
