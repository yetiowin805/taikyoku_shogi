//! 5×3×3 file/back/tropism grid (45 agents).
//!
//! Axes:
//! - File F{edge}C{center}: even, 0.9↔1.1, 0.8↔1.2, 0.7↔1.5, 0.5↔2.0
//! - Back B ∈ {60, 75, 90}% (fast rank PST back only; promo/opp-half stay seed)
//! - Tropism T ∈ {10, 15, 20} → `eg_tropism_scale` 1.0 / 1.5 / 2.0
//!
//! Seed cell `F100C100B75T15` is a byte-copy of the seed.

use crate::eval::{
    seed_file_factors, seed_rank_factors_fast_params, EvalCheckpoint, EvalWeights,
};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/file-pst-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// (edge, center) file-PST regimes.
pub const FILE_REGIMES: [(f32, f32); 5] = [
    (1.0, 1.0),
    (0.9, 1.1),
    (0.8, 1.2),
    (0.7, 1.5),
    (0.5, 2.0),
];
/// Fast rank-PST back anchors (B60 / B75 / B90).
pub const BACK_FACTORS: [f32; 3] = [0.60, 0.75, 0.90];
/// `eg_tropism_scale` values (T10 / T15 / T20).
pub const TROPISM_SCALES: [f32; 3] = [1.0, 1.5, 2.0];

#[derive(Debug, Clone)]
pub struct FilePstGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
}

impl Default for FilePstGridConfig {
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
    pub file_edge: f32,
    pub file_center: f32,
    pub back_factor: f32,
    pub eg_tropism_scale: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn cell_id(edge: f32, center: f32, back: f32, trop: f32) -> String {
    format!(
        "F{}C{}B{}T{}",
        pct(edge),
        pct(center),
        pct(back),
        trop_tag(trop)
    )
}

fn pct(mult: f32) -> u32 {
    (mult * 100.0).round() as u32
}

fn trop_tag(scale: f32) -> u32 {
    (scale * 10.0).round() as u32
}

/// Apply file PST, fast-rank back, and tropism scale; leave material/slow PST alone.
pub fn apply_file_pst_cell(
    base: &EvalWeights,
    edge: f32,
    center: f32,
    back: f32,
    tropism_scale: f32,
) -> EvalWeights {
    let mut w = base.clone();
    w.file_factor = seed_file_factors(edge, center).to_vec();
    let fast = seed_rank_factors_fast_params(back, 0.5, 1.2).to_vec();
    w.rank_factor_fast = fast.clone();
    w.rank_factor = fast;
    w.eg_tropism_scale = tropism_scale;
    w
}

/// Write 45 checkpoints + manifest.json + grid.json under `out_dir`.
pub fn run_file_pst_grid(cfg: &FilePstGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    let mut cells = Vec::with_capacity(45);
    let mut entrants = Vec::with_capacity(45);
    for &(edge, center) in &FILE_REGIMES {
        for &back in &BACK_FACTORS {
            for &trop in &TROPISM_SCALES {
                let id = cell_id(edge, center, back, trop);
                let model_path = cfg.out_dir.join(format!("{id}.json"));
                let is_seed_cell = (edge - 1.0).abs() < 1e-6
                    && (center - 1.0).abs() < 1e-6
                    && (back - 0.75).abs() < 1e-6
                    && (trop - 1.5).abs() < 1e-6;
                if is_seed_cell {
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
                    cp.weights = apply_file_pst_cell(base, edge, center, back, trop);
                    cp.save_path(&model_path)
                        .map_err(|e| format!("save {}: {e}", model_path.display()))?;
                }
                let model = model_path.display().to_string();
                cells.push(GridCell {
                    id: id.clone(),
                    model: model.clone(),
                    file_edge: edge,
                    file_center: center,
                    back_factor: back,
                    eg_tropism_scale: trop,
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
    use crate::eval::EvalWeights;

    #[test]
    fn cell_ids_cover_45() {
        let mut ids = Vec::new();
        for &(e, c) in &FILE_REGIMES {
            for &b in &BACK_FACTORS {
                for &t in &TROPISM_SCALES {
                    ids.push(cell_id(e, c, b, t));
                }
            }
        }
        assert_eq!(ids.len(), 45);
        assert!(ids.contains(&"F100C100B75T15".to_string()));
        assert!(ids.contains(&"F50C200B60T10".to_string()));
        assert!(ids.contains(&"F70C150B90T20".to_string()));
        assert_eq!(trop_tag(2.0), 20);
    }

    #[test]
    fn apply_touches_file_back_tropism() {
        let seed = EvalWeights::seed();
        let w = apply_file_pst_cell(&seed, 0.5, 2.0, 0.60, 2.0);
        assert!((w.file_factor[0] - 0.5).abs() < 1e-5);
        assert!(w.file_factor[18] > 1.9);
        assert!((w.rank_factor_fast[0] - 0.60).abs() < 1e-5);
        assert!((w.eg_tropism_scale - 2.0).abs() < 1e-6);
        assert_eq!(w.rank_factor_slow, seed.rank_factor_slow);
    }
}
