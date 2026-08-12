//! 3×3×3 fast rank-PST grid: promo × opp-half mix × back.
//!
//! Axes (applied to `rank_factor_fast` / legacy `rank_factor`; slow PST unchanged):
//! - Promo P ∈ {110, 120, 130}%
//! - Opp-half H ∈ {25, 50, 75}% of the mid→promo gap
//!   (e.g. P130 H25 → opp plateau 107.5%)
//! - Back B ∈ {25, 50, 75}% (linear to 100% at pawn start)
//!
//! Seed cell `P120H50B75` matches the baked fast PST and is a byte-copy of the seed.

use crate::eval::{seed_rank_factors_fast_params, EvalCheckpoint, EvalWeights};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/pst-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// Promo-zone factors (P110 / P120 / P130).
pub const PROMO_FACTORS: [f32; 3] = [1.10, 1.20, 1.30];
/// Opp-half as a fraction of (promo − mid) (H25 / H50 / H75).
pub const OPP_HALF_FRACS: [f32; 3] = [0.25, 0.50, 0.75];
/// Back-rank factors (B25 / B50 / B75).
pub const BACK_FACTORS: [f32; 3] = [0.25, 0.50, 0.75];

#[derive(Debug, Clone)]
pub struct PstGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
}

impl Default for PstGridConfig {
    fn default() -> Self {
        Self {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridCell {
    pub id: String,
    pub model: String,
    pub promo_factor: f32,
    pub opp_half_frac: f32,
    pub back_factor: f32,
    pub opp_half_factor: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn cell_id(promo_pct: u32, opp_half_pct: u32, back_pct: u32) -> String {
    format!("P{promo_pct}H{opp_half_pct}B{back_pct}")
}

fn pct(mult: f32) -> u32 {
    (mult * 100.0).round() as u32
}

pub fn opp_half_factor(promo_factor: f32, opp_half_frac: f32) -> f32 {
    1.0 + opp_half_frac * (promo_factor - 1.0)
}

/// Replace fast (and legacy) rank PST; leave material / slow PST / tropism alone.
pub fn apply_fast_pst_params(
    base: &EvalWeights,
    back: f32,
    opp_half_frac: f32,
    promo_factor: f32,
) -> EvalWeights {
    let mut w = base.clone();
    let fast = seed_rank_factors_fast_params(back, opp_half_frac, promo_factor).to_vec();
    w.rank_factor_fast = fast.clone();
    w.rank_factor = fast;
    w
}

/// Write 27 checkpoints + manifest.json + grid.json under `out_dir`.
pub fn run_pst_grid(cfg: &PstGridConfig) -> Result<(TourneyManifest, GridFile), String> {
    if !cfg.seed_model.is_file() {
        return Err(format!(
            "missing seed model {} (export-seed or copy checkpoint first)",
            cfg.seed_model.display()
        ));
    }
    let base_cp = EvalCheckpoint::load_path(&cfg.seed_model)?;
    let base = &base_cp.weights;

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(27);
    let mut entrants = Vec::with_capacity(27);
    for &promo in &PROMO_FACTORS {
        for &h_frac in &OPP_HALF_FRACS {
            for &back in &BACK_FACTORS {
                let id = cell_id(pct(promo), pct(h_frac), pct(back));
                let model_path = cfg.out_dir.join(format!("{id}.json"));
                let is_center = (promo - 1.2).abs() < 1e-6
                    && (h_frac - 0.5).abs() < 1e-6
                    && (back - 0.75).abs() < 1e-6;
                if is_center {
                    fs::copy(&cfg.seed_model, &model_path).map_err(|e| {
                        format!(
                            "copy {} → {}: {e}",
                            cfg.seed_model.display(),
                            model_path.display()
                        )
                    })?;
                } else {
                    let mut cp = base_cp.clone();
                    cp.name = id.clone();
                    cp.weights = apply_fast_pst_params(base, back, h_frac, promo);
                    cp.save_path(&model_path)
                        .map_err(|e| format!("save {}: {e}", model_path.display()))?;
                }
                let model = model_path.display().to_string();
                cells.push(GridCell {
                    id: id.clone(),
                    model: model.clone(),
                    promo_factor: promo,
                    opp_half_frac: h_frac,
                    back_factor: back,
                    opp_half_factor: opp_half_factor(promo, h_frac),
                });
                entrants.push(TourneyEntrant { id, model });
            }
        }
    }

    let grid = GridFile {
        seed_model: cfg.seed_model.display().to_string(),
        cells,
    };
    let grid_path = cfg.out_dir.join("grid.json");
    fs::write(
        &grid_path,
        serde_json::to_string_pretty(&grid).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {}: {e}", grid_path.display()))?;

    let manifest = TourneyManifest { entrants };
    let man_path = cfg.out_dir.join("manifest.json");
    fs::write(
        &man_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {}: {e}", man_path.display()))?;

    Ok((manifest, grid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{seed_rank_factors_fast, EvalWeights, RANK_OPPONENT_HALF, RANK_PST_PROMO};

    #[test]
    fn center_params_match_seed_fast_pst() {
        let built = seed_rank_factors_fast_params(0.75, 0.5, 1.2);
        let seed = seed_rank_factors_fast();
        for i in 0..36 {
            assert!(
                (built[i] - seed[i]).abs() < 1e-6,
                "rank {i}: built={} seed={}",
                built[i],
                seed[i]
            );
        }
    }

    #[test]
    fn opp_half_interpolates_mid_to_promo() {
        assert!((opp_half_factor(1.30, 0.25) - 1.075).abs() < 1e-6);
        assert!((opp_half_factor(1.30, 0.50) - 1.15).abs() < 1e-6);
        assert!((opp_half_factor(1.20, 0.50) - 1.10).abs() < 1e-6);
    }

    #[test]
    fn apply_only_touches_fast_pst() {
        let seed = EvalWeights::seed();
        let w = apply_fast_pst_params(&seed, 0.25, 0.75, 1.30);
        assert!((w.rank_factor_fast[0] - 0.25).abs() < 1e-6);
        assert!(
            (w.rank_factor_fast[RANK_OPPONENT_HALF as usize] - opp_half_factor(1.30, 0.75)).abs()
                < 1e-6
        );
        assert!((w.rank_factor_fast[RANK_PST_PROMO as usize] - 1.30).abs() < 1e-6);
        assert_eq!(w.rank_factor_slow, seed.rank_factor_slow);
        assert!((w.piece_value(crate::piece::PieceType::HookMover)
            - seed.piece_value(crate::piece::PieceType::HookMover))
            .abs()
            < 1e-3);
    }

    #[test]
    fn cell_ids_cover_27() {
        let mut ids = Vec::new();
        for &p in &PROMO_FACTORS {
            for &h in &OPP_HALF_FRACS {
                for &b in &BACK_FACTORS {
                    ids.push(cell_id(pct(p), pct(h), pct(b)));
                }
            }
        }
        assert_eq!(ids.len(), 27);
        assert!(ids.contains(&"P120H50B75".to_string()));
        assert!(ids.contains(&"P110H25B25".to_string()));
        assert!(ids.contains(&"P130H75B75".to_string()));
    }
}
