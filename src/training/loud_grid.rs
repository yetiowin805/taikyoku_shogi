//! 3×3 material grid over range two-movers × range capturers (incl. FreeKing).

use crate::eval::{
    is_range_capturer, is_range_two_mover, EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES,
};
use crate::piece::PieceType;
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_OUT_DIR: &str = "models/loud-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

pub const MULTS: [f32; 3] = [0.5, 1.0, 1.5];

#[derive(Debug, Clone)]
pub struct LoudGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
}

impl Default for LoudGridConfig {
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
    pub two_mover_mult: f32,
    pub capturer_mult: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub two_mover_pieces: Vec<String>,
    pub capturer_pieces: Vec<String>,
    pub cells: Vec<GridCell>,
}

pub fn range_two_mover_types() -> Vec<PieceType> {
    ALL_PIECE_TYPES
        .iter()
        .copied()
        .filter(|pt| is_range_two_mover(*pt))
        .collect()
}

pub fn range_capturer_types() -> Vec<PieceType> {
    ALL_PIECE_TYPES
        .iter()
        .copied()
        .filter(|pt| is_range_capturer(*pt))
        .collect()
}

pub fn cell_id(two_mover_pct: u32, capturer_pct: u32) -> String {
    format!("T{two_mover_pct}C{capturer_pct}")
}

fn pct(mult: f32) -> u32 {
    (mult * 100.0).round() as u32
}

/// Scale both loud classes on a clone of `base`.
pub fn apply_loud_mults(
    base: &EvalWeights,
    two_mover_mult: f32,
    capturer_mult: f32,
) -> EvalWeights {
    let mut w = base.clone();
    for pt in range_two_mover_types() {
        let v = base.piece_value(pt) * two_mover_mult;
        w.piece.insert(pt, v);
    }
    for pt in range_capturer_types() {
        let v = base.piece_value(pt) * capturer_mult;
        w.piece.insert(pt, v);
    }
    w.rebuild_piece_value_table();
    w
}

/// Write 9 checkpoints + manifest.json + grid.json under `out_dir`.
pub fn run_loud_grid(cfg: &LoudGridConfig) -> Result<(TourneyManifest, GridFile), String> {
    if !cfg.seed_model.is_file() {
        return Err(format!(
            "missing seed model {} (copy existing checkpoint; do not regenerate)",
            cfg.seed_model.display()
        ));
    }
    let base_cp = EvalCheckpoint::load_path(&cfg.seed_model)?;
    let base = &base_cp.weights;
    let two_movers = range_two_mover_types();
    let capturers = range_capturer_types();
    if two_movers.is_empty() {
        return Err("no range two-movers found".into());
    }
    if capturers.is_empty() {
        return Err("no range capturers found".into());
    }
    if !capturers.contains(&PieceType::FreeKing) {
        return Err("FreeKing must be in range capturer class".into());
    }

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(9);
    let mut entrants = Vec::with_capacity(9);
    for &tm in &MULTS {
        for &cm in &MULTS {
            let id = cell_id(pct(tm), pct(cm));
            let model_path = cfg.out_dir.join(format!("{id}.json"));
            if (tm - 1.0).abs() < 1e-6 && (cm - 1.0).abs() < 1e-6 {
                // Center cell: byte-copy seed so it matches ab-seed exactly.
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
                cp.weights = apply_loud_mults(base, tm, cm);
                cp.save_path(&model_path)
                    .map_err(|e| format!("save {}: {e}", model_path.display()))?;
            }
            let model = model_path.display().to_string();
            cells.push(GridCell {
                id: id.clone(),
                model: model.clone(),
                two_mover_mult: tm,
                capturer_mult: cm,
            });
            entrants.push(TourneyEntrant { id, model });
        }
    }

    let grid = GridFile {
        seed_model: cfg.seed_model.display().to_string(),
        two_mover_pieces: two_movers.iter().map(|pt| format!("{pt:?}")).collect(),
        capturer_pieces: capturers.iter().map(|pt| format!("{pt:?}")).collect(),
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
    fn center_matches_seed_scaled_types() {
        let seed = EvalWeights::seed();
        let mid = apply_loud_mults(&seed, 1.0, 1.0);
        for pt in range_two_mover_types() {
            assert!((mid.piece_value(pt) - seed.piece_value(pt)).abs() < 1e-3);
        }
        for pt in range_capturer_types() {
            assert!((mid.piece_value(pt) - seed.piece_value(pt)).abs() < 1e-3);
        }
    }

    #[test]
    fn corner_scales_classes_independently() {
        let seed = EvalWeights::seed();
        let w = apply_loud_mults(&seed, 0.5, 1.5);
        assert!(
            (w.piece_value(PieceType::Tengu) - seed.piece_value(PieceType::Tengu) * 0.5).abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::GreatGeneral)
                - seed.piece_value(PieceType::GreatGeneral) * 1.5)
                .abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::FreeKing) - seed.piece_value(PieceType::FreeKing) * 1.5)
                .abs()
                < 1e-2
        );
        // Lion untouched.
        assert!((w.piece_value(PieceType::Lion) - seed.piece_value(PieceType::Lion)).abs() < 1e-3);
    }

    #[test]
    fn cell_ids_cover_grid() {
        let mut ids = Vec::new();
        for &tm in &MULTS {
            for &cm in &MULTS {
                ids.push(cell_id(pct(tm), pct(cm)));
            }
        }
        assert_eq!(ids.len(), 9);
        assert!(ids.contains(&"T100C100".to_string()));
        assert!(ids.contains(&"T50C150".to_string()));
        assert!(ids.contains(&"T150C50".to_string()));
    }
}
