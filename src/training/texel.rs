//! Texel-style logistic fit on derived piece-count features.

use crate::eval::{
    is_range_capturer, is_range_two_mover, EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES,
};
use crate::piece::{Color, PieceType};
use crate::training::featurize::{load_labeled_dir, LabeledPosition};
use crate::training::mobility_seed::{run_mobility_seed, MobilitySeedConfig};
use crate::training::paths;
use std::collections::HashMap;
use std::path::Path;

/// Where piece weights start before fitting.
#[derive(Debug, Clone)]
pub enum TexelInit {
    /// Hand-authored [`EvalWeights::seed`].
    Seed,
    /// Mobility Monte Carlo prior (`models/ab-mobility-seed.json`, or generate).
    Mobility,
    /// Load an existing checkpoint JSON.
    Path(String),
}

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
    pub init: TexelInit,
    /// Drop draw-labeled rows (`result == 0.5`).
    pub drop_draws: bool,
    /// Keep only plies at or after this fraction of each game's max ply (0..=1).
    pub late_frac: f32,
    /// Optimize in log-space (`θ = log w`) so relative % moves are the natural step.
    pub log_space: bool,
    /// Divide effective LR by K. Off by default: on mix data K is ~1e-4 and
    /// `lr/k` becomes a 500-step. Cap at [`LR_SCALE_K_MAX`] if enabled.
    pub lr_scale_by_k: bool,
    /// Divide all piece values by Pawn after fit so Pawn = 1.
    pub renormalize_pawn: bool,
    /// Train only range two-movers and range capturers; leave the mid table at `--init`.
    pub only_large: bool,
    /// Log-space L2 toward `--init` (`λ · (log w − log w0)²`). 0 = no prior.
    pub l2: f32,
}

impl Default for TexelFitConfig {
    fn default() -> Self {
        Self {
            features_dir: paths::DERIVED_POSITIONS.to_string(),
            out_model: "models/ab-texel.json".to_string(),
            iterations: 2500,
            learning_rate: 0.05,
            k: 0.1,
            fit_k: true,
            k_target_abs: 1.0,
            init: TexelInit::Seed,
            drop_draws: false,
            late_frac: 0.0,
            log_space: true,
            lr_scale_by_k: false,
            renormalize_pawn: true,
            only_large: true,
            l2: 0.5,
        }
    }
}

/// Floor for [`calibrate_k`]. 0.01 was too high for Hook-scale material (mean
/// |e| ~ 10⁴) and forced overconfident CE, then a collapse of the loud pieces.
const K_MIN: f32 = 1e-5;
const K_MAX: f32 = 2.0;
/// When `lr_scale_by_k`, never let `lr/k` exceed this.
const LR_SCALE_K_MAX: f32 = 0.25;

/// Loud pieces we have been grid-searching: range two-movers + capturing-range (incl. FreeKing).
pub fn is_texel_large_piece(pt: PieceType) -> bool {
    is_range_two_mover(pt) || is_range_capturer(pt)
}

#[derive(Debug, Clone)]
pub struct TexelFitStats {
    pub k: f32,
    pub loss_before: f64,
    pub loss_after: f64,
    pub max_abs_delta: f32,
    pub mean_abs_delta: f32,
    pub max_pct_delta: f32,
    pub mean_pct_delta: f32,
    pub n_raw: usize,
    pub n_used: usize,
    pub n_draws_dropped: usize,
    pub n_early_dropped: usize,
    /// How many piece types were updated (`only_large` → two-movers + capturers).
    pub n_trained: usize,
    /// Fraction of used rows where sign(seed_eval) agrees with win/loss label.
    pub sign_agreement: f64,
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
    (target / mean_abs).clamp(K_MIN, K_MAX)
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

fn sign_agreement(rows: &[LabeledPosition], weights: &[f32]) -> f64 {
    let mut ok = 0usize;
    let mut n = 0usize;
    for row in rows {
        if (row.result - 0.5).abs() < 1e-6 {
            continue;
        }
        let e = eval_from_diff(weights, &row.piece_diff);
        let pred_black = e > 0.0;
        let label_black = row.result > 0.5;
        if pred_black == label_black {
            ok += 1;
        }
        n += 1;
    }
    if n == 0 {
        0.0
    } else {
        ok as f64 / n as f64
    }
}

/// Drop draws and early plies; returns (filtered, n_draws_dropped, n_early_dropped).
pub fn filter_labeled_rows(
    rows: &[LabeledPosition],
    drop_draws: bool,
    late_frac: f32,
) -> (Vec<LabeledPosition>, usize, usize) {
    let mut max_ply: HashMap<&str, usize> = HashMap::new();
    for row in rows {
        let e = max_ply.entry(row.game_id.as_str()).or_insert(0);
        *e = (*e).max(row.ply);
    }
    let late = late_frac.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity(rows.len());
    let mut n_draws = 0usize;
    let mut n_early = 0usize;
    for row in rows {
        if drop_draws && (row.result - 0.5).abs() < 1e-6 {
            n_draws += 1;
            continue;
        }
        let game_max = *max_ply.get(row.game_id.as_str()).unwrap_or(&row.ply);
        let thresh = ((game_max as f32) * late).floor() as usize;
        if row.ply < thresh {
            n_early += 1;
            continue;
        }
        out.push(row.clone());
    }
    (out, n_draws, n_early)
}

fn resolve_init_weights(init: &TexelInit) -> Result<EvalWeights, String> {
    match init {
        TexelInit::Seed => Ok(EvalWeights::seed()),
        TexelInit::Path(p) => {
            let cp = EvalCheckpoint::load_path(p)?;
            Ok(cp.weights)
        }
        TexelInit::Mobility => {
            let default_path = "models/ab-mobility-seed.json";
            if Path::new(default_path).is_file() {
                let cp = EvalCheckpoint::load_path(default_path)?;
                return Ok(cp.weights);
            }
            // Generate once if missing (writes default path).
            let cfg = MobilitySeedConfig::default();
            let (cp, _) = run_mobility_seed(&cfg)?;
            Ok(cp.weights)
        }
    }
}

fn piece_vec(weights: &EvalWeights) -> Vec<f32> {
    ALL_PIECE_TYPES
        .iter()
        .map(|pt| weights.piece_value(*pt).max(0.05))
        .collect()
}

fn pawn_index() -> usize {
    ALL_PIECE_TYPES
        .iter()
        .position(|p| *p == PieceType::Pawn)
        .expect("Pawn in ALL_PIECE_TYPES")
}

/// Fit piece values via logistic regression (gradient descent).
///
/// Default: only range two-movers and range capturers move. Pawns, golds, royals,
/// and the rest of the mid table stay at `--init` so a long-game CE collapse
/// cannot rewrite them. Pass `only_large: false` / `--all-pieces` to train every
/// type. Rank PST / tropism / `two_mover_mob_k` stay at init either way.
/// Terminal 0-royals is not learned — [`EvalWeights::mate_score`] / `get_winner`
/// already treat that as ±∞.
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

    let n_raw = rows.len();
    let (filtered, n_draws_dropped, n_early_dropped) =
        filter_labeled_rows(rows, cfg.drop_draws, cfg.late_frac);
    if filtered.is_empty() {
        return Err(
            "No positions left after label filters (draws / late ply). Relax --late-frac or --keep-draws."
                .into(),
        );
    }
    let rows = &filtered;

    let mut seed = resolve_init_weights(&cfg.init)?;
    let w0 = piece_vec(&seed);
    let mut w = w0.clone();
    let trained: Vec<bool> = ALL_PIECE_TYPES
        .iter()
        .map(|pt| !cfg.only_large || is_texel_large_piece(*pt))
        .collect();
    let n_trained = trained.iter().filter(|t| **t).count();
    if n_trained == 0 {
        return Err("No trainable piece types (only_large matched nothing)".into());
    }

    let mut k = if cfg.fit_k {
        calibrate_k(rows, &w, cfg.k_target_abs)
    } else {
        cfg.k
    };

    let loss_before = mean_cross_entropy(rows, &w, k);
    let agree_before = sign_agreement(rows, &w);
    let base_lr = cfg.learning_rate;

    for iter in 0..cfg.iterations {
        // Re-scale K as weights move so the logistic stays in a useful regime.
        if cfg.fit_k && iter > 0 && iter % 25 == 0 {
            k = calibrate_k(rows, &w, cfg.k_target_abs);
        }

        let lr = if cfg.lr_scale_by_k {
            (base_lr / k.max(K_MIN)).min(LR_SCALE_K_MAX)
        } else {
            base_lr
        };

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
        if cfg.log_space {
            // θ = log w; ∂L/∂θ_i = (∂L/∂w_i) · w_i
            for i in 0..w.len() {
                if !trained[i] {
                    continue;
                }
                let g_w = grad[i] * inv_n;
                let mut g_theta = g_w * w[i];
                if cfg.l2 > 0.0 {
                    g_theta += cfg.l2 * (w[i].max(0.05).ln() - w0[i].max(0.05).ln());
                }
                let theta = w[i].max(0.05).ln() - lr * g_theta;
                w[i] = theta.exp().max(0.05);
            }
        } else {
            for i in 0..w.len() {
                if !trained[i] {
                    continue;
                }
                let mut g = grad[i] * inv_n;
                if cfg.l2 > 0.0 {
                    g += cfg.l2 * (w[i] - w0[i]);
                }
                w[i] -= lr * g;
                if w[i] < 0.05 {
                    w[i] = 0.05;
                }
            }
        }
    }

    // Deltas vs init before pawn renorm, over trained types only.
    let mut max_abs_delta = 0.0f32;
    let mut sum_abs_delta = 0.0f32;
    let mut max_pct_delta = 0.0f32;
    let mut sum_pct_delta = 0.0f32;
    for i in 0..w.len() {
        if !trained[i] {
            continue;
        }
        let d = (w[i] - w0[i]).abs();
        max_abs_delta = max_abs_delta.max(d);
        sum_abs_delta += d;
        let base = w0[i].abs().max(1e-3);
        let pct = 100.0 * d / base;
        max_pct_delta = max_pct_delta.max(pct);
        sum_pct_delta += pct;
    }
    let n_tr = n_trained.max(1) as f32;
    let mean_abs_delta = sum_abs_delta / n_tr;
    let mean_pct_delta = sum_pct_delta / n_tr;

    let loss_after = mean_cross_entropy(rows, &w, k);
    let agree_after = sign_agreement(rows, &w);

    let init_pawn = w0[pawn_index()].max(1e-3);
    let fit_pawn_pre = w[pawn_index()].max(1e-3);

    eprintln!(
        "texel filter: raw={n_raw} used={} draws_dropped={n_draws_dropped} early_dropped={n_early_dropped} trained={n_trained}/{}",
        rows.len(),
        ALL_PIECE_TYPES.len()
    );
    eprintln!(
        "texel agreement: before={agree_before:.3} after={agree_after:.3}  max%Δ={max_pct_delta:.1} mean%Δ={mean_pct_delta:.1}"
    );
    let loud = [
        PieceType::GreatGeneral,
        PieceType::ViceGeneral,
        PieceType::BishopGeneral,
        PieceType::Peacock,
        PieceType::HookMover,
        PieceType::FreeKing,
    ];
    for pt in loud {
        if let Some(i) = ALL_PIECE_TYPES.iter().position(|p| *p == pt) {
            let r0 = w0[i] / init_pawn;
            let r1 = w[i] / fit_pawn_pre;
            let pct = 100.0 * (r1 - r0) / r0.max(1e-3);
            eprintln!(
                "  {:?} /Pawn: {:.3} → {:.3} ({pct:+.1}% relative)",
                pt, r0, r1
            );
        }
    }

    // Renorm would rewrite frozen mid-table values. Skip when only_large.
    if cfg.renormalize_pawn && !cfg.only_large {
        let pi = pawn_index();
        let pawn = w[pi].max(0.05);
        for v in &mut w {
            *v /= pawn;
        }
    }

    for (i, &pt) in ALL_PIECE_TYPES.iter().enumerate() {
        seed.piece.insert(pt, w[i]);
    }
    seed.rebuild_piece_value_table();

    let stats = TexelFitStats {
        k,
        loss_before,
        loss_after,
        max_abs_delta,
        mean_abs_delta,
        max_pct_delta,
        mean_pct_delta,
        n_raw,
        n_used: rows.len(),
        n_draws_dropped,
        n_early_dropped,
        n_trained,
        sign_agreement: agree_after,
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
            let ply = 80 + (i % 20);
            rows.push(LabeledPosition {
                format_version: FEATURE_FORMAT_VERSION,
                game_id: format!("syn-{}", i / 4),
                ply,
                result: if black_ahead { 1.0 } else { 0.0 },
                turn: Color::Black,
                piece_diff: diff,
                seed_eval: if black_ahead { 5.0 } else { -5.0 },
            });
        }
        rows
    }

    #[test]
    fn texel_large_is_two_movers_and_capturers() {
        assert!(is_texel_large_piece(PieceType::HookMover));
        assert!(is_texel_large_piece(PieceType::Capricorn));
        assert!(is_texel_large_piece(PieceType::Tengu));
        assert!(is_texel_large_piece(PieceType::Peacock));
        assert!(is_texel_large_piece(PieceType::GreatGeneral));
        assert!(is_texel_large_piece(PieceType::ViceGeneral));
        assert!(is_texel_large_piece(PieceType::FreeKing));
        assert!(is_texel_large_piece(PieceType::FierceDragon));
        assert!(!is_texel_large_piece(PieceType::Pawn));
        assert!(!is_texel_large_piece(PieceType::GoldGeneral));
        assert!(!is_texel_large_piece(PieceType::King));
        assert!(!is_texel_large_piece(PieceType::Lion));
    }

    #[test]
    fn calibrate_k_allows_tiny_k_for_hook_scale() {
        let hi = ALL_PIECE_TYPES
            .iter()
            .position(|p| *p == PieceType::HookMover)
            .expect("Hook");
        let mut rows = Vec::new();
        for i in 0..8 {
            let mut diff = vec![0.0f32; ALL_PIECE_TYPES.len()];
            diff[hi] = if i % 2 == 0 { 1.0 } else { -1.0 };
            rows.push(LabeledPosition {
                format_version: FEATURE_FORMAT_VERSION,
                game_id: format!("k-{i}"),
                ply: 10,
                result: 1.0,
                turn: Color::Black,
                piece_diff: diff,
                seed_eval: 5000.0,
            });
        }
        let mut w = vec![0.05f32; ALL_PIECE_TYPES.len()];
        w[hi] = 5000.0;
        let k = calibrate_k(&rows, &w, 1.0);
        assert!(
            k < 0.01,
            "hook-scale material must not clamp K to 0.01, got {k}"
        );
        assert!((k - 1.0 / 5000.0).abs() < 1e-6);
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
    fn filter_drops_draws_and_early() {
        let mut rows = synthetic_rows(8);
        rows[0].result = 0.5;
        rows[0].ply = 1;
        rows[1].ply = 1;
        let (f, nd, ne) = filter_labeled_rows(&rows, true, 0.75);
        assert!(nd >= 1);
        assert!(ne >= 1);
        assert!(f.len() < rows.len());
        assert!(f.iter().all(|r| (r.result - 0.5).abs() > 1e-6));
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
            iterations: 200,
            learning_rate: 0.1,
            k: 0.1,
            fit_k: true,
            k_target_abs: 1.0,
            init: TexelInit::Seed,
            drop_draws: true,
            late_frac: 0.0, // keep all plies for synthetic
            log_space: true,
            lr_scale_by_k: true,
            renormalize_pawn: true,
            only_large: false,
            l2: 0.0,
        };
        let seed_pawn = EvalWeights::seed().piece_value(PieceType::Pawn);
        let (cp, stats) = fit_texel_on_rows(&cfg, &rows).expect("fit");
        assert!(
            stats.max_pct_delta > 1.0 || stats.max_abs_delta > 0.05,
            "expected weights to move, max_abs={} max%={}",
            stats.max_abs_delta,
            stats.max_pct_delta
        );
        // Synthetic data: more pawns ⇒ win. After renorm pawn=1; relative to other
        // pieces pawn should stay meaningful — check CE improved.
        assert!(
            stats.loss_after <= stats.loss_before + 1e-6,
            "loss rose: before={} after={}",
            stats.loss_before,
            stats.loss_after
        );
        let fitted_pawn = cp.weights.piece_value(PieceType::Pawn);
        assert!(
            (fitted_pawn - 1.0).abs() < 1e-3,
            "expected pawn≈1 after renorm, got {fitted_pawn} (seed was {seed_pawn})"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn hook_idx() -> usize {
        ALL_PIECE_TYPES
            .iter()
            .position(|p| *p == PieceType::HookMover)
            .expect("HookMover")
    }

    fn synthetic_hook_rows(n: usize) -> Vec<LabeledPosition> {
        let hi = hook_idx();
        (0..n)
            .map(|i| {
                let black_ahead = i % 2 == 0;
                let mut diff = vec![0.0f32; ALL_PIECE_TYPES.len()];
                diff[hi] = if black_ahead { 1.0 } else { -1.0 };
                LabeledPosition {
                    format_version: FEATURE_FORMAT_VERSION,
                    game_id: format!("syn-hook-{}", i / 4),
                    ply: 80,
                    // Opposite of material: extra Hook loses, so the fit must move Hook.
                    result: if black_ahead { 0.0 } else { 1.0 },
                    turn: Color::Black,
                    piece_diff: diff,
                    seed_eval: if black_ahead { 1.0 } else { -1.0 },
                }
            })
            .collect()
    }

    #[test]
    fn only_large_freezes_mid_table_and_moves_hook() {
        let dir = std::env::temp_dir().join(format!(
            "taikyoku-texel-large-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let rows = synthetic_hook_rows(64);
        let seed = EvalWeights::seed();
        let cfg = TexelFitConfig {
            features_dir: dir.to_string_lossy().into(),
            out_model: dir.join("out.json").to_string_lossy().into(),
            iterations: 80,
            learning_rate: 0.05,
            // Seed Hook is ~6k; k=0.1 saturates sigmoid and freezes the gradient.
            k: 1.0 / 6237.0,
            fit_k: false,
            k_target_abs: 1.0,
            init: TexelInit::Seed,
            drop_draws: true,
            late_frac: 0.0,
            log_space: true,
            lr_scale_by_k: false,
            renormalize_pawn: true,
            only_large: true,
            l2: 0.0,
        };
        let (cp, stats) = fit_texel_on_rows(&cfg, &rows).expect("fit");
        assert!(stats.n_trained > 0);
        assert!(stats.n_trained < ALL_PIECE_TYPES.len());
        assert!(
            (cp.weights.piece_value(PieceType::Pawn) - seed.piece_value(PieceType::Pawn)).abs()
                < 1e-5,
            "pawn must stay at init"
        );
        assert!(
            (cp.weights.piece_value(PieceType::GoldGeneral) - seed.piece_value(PieceType::GoldGeneral))
                .abs()
                < 1e-5,
            "gold must stay at init"
        );
        let hook0 = seed.piece_value(PieceType::HookMover);
        let hook1 = cp.weights.piece_value(PieceType::HookMover);
        assert!(
            hook1 < hook0 - 1.0,
            "hook should fall when extra Hook loses: {hook0} → {hook1}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
