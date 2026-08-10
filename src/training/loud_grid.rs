//! 3×3×3 material grid: HookMover × Capricorn × other range two-movers.
//!
//! Default grid (percent of seed):
//! - HookMover H ∈ {90, 100, 110}
//! - Capricorn C ∈ {80, 100, 120}
//! - Other two-movers O ∈ {80, 100, 110} (excl. Hook + Capricorn)
//!
//! Capturers stay at seed. Center `H100C100O100` is a byte-copy of the seed.

use crate::eval::{is_range_two_mover, EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::piece::PieceType;
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/loud-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// HookMover multipliers (H90 / H100 / H110).
pub const HOOK_MOVER_MULTS: [f32; 3] = [0.90, 1.0, 1.10];
/// Capricorn multipliers (C80 / C100 / C120).
pub const CAPRICORN_MULTS: [f32; 3] = [0.80, 1.0, 1.20];
/// Other range two-mover multipliers (O80 / O100 / O110): Tengu, Peacock, …
pub const OTHER_TWO_MOVER_MULTS: [f32; 3] = [0.80, 1.0, 1.10];

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
    pub hook_mover_mult: f32,
    pub capricorn_mult: f32,
    pub other_two_mover_mult: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub hook_mover_pieces: Vec<String>,
    pub capricorn_pieces: Vec<String>,
    pub other_two_mover_pieces: Vec<String>,
    pub cells: Vec<GridCell>,
}

pub fn hook_mover_types() -> Vec<PieceType> {
    vec![PieceType::HookMover]
}

pub fn capricorn_types() -> Vec<PieceType> {
    vec![PieceType::Capricorn]
}

/// Range two-movers excluding HookMover and Capricorn.
pub fn other_range_two_mover_types() -> Vec<PieceType> {
    ALL_PIECE_TYPES
        .iter()
        .copied()
        .filter(|pt| {
            is_range_two_mover(*pt)
                && *pt != PieceType::HookMover
                && *pt != PieceType::Capricorn
        })
        .collect()
}

pub fn cell_id(hook_pct: u32, capricorn_pct: u32, other_pct: u32) -> String {
    format!("H{hook_pct}C{capricorn_pct}O{other_pct}")
}

fn pct(mult: f32) -> u32 {
    (mult * 100.0).round() as u32
}

/// Scale Hook, Capricorn, and other two-movers independently on a clone of `base`.
pub fn apply_hco_mults(
    base: &EvalWeights,
    hook_mover_mult: f32,
    capricorn_mult: f32,
    other_two_mover_mult: f32,
) -> EvalWeights {
    let mut w = base.clone();
    for pt in hook_mover_types() {
        w.piece.insert(pt, base.piece_value(pt) * hook_mover_mult);
    }
    for pt in capricorn_types() {
        w.piece.insert(pt, base.piece_value(pt) * capricorn_mult);
    }
    for pt in other_range_two_mover_types() {
        w.piece
            .insert(pt, base.piece_value(pt) * other_two_mover_mult);
    }
    w.rebuild_piece_value_table();
    w
}

/// Write 27 checkpoints + manifest.json + grid.json under `out_dir`.
pub fn run_loud_grid(cfg: &LoudGridConfig) -> Result<(TourneyManifest, GridFile), String> {
    if !cfg.seed_model.is_file() {
        return Err(format!(
            "missing seed model {} (copy existing checkpoint; do not regenerate)",
            cfg.seed_model.display()
        ));
    }
    let base_cp = EvalCheckpoint::load_path(&cfg.seed_model)?;
    let base = &base_cp.weights;
    let hooks = hook_mover_types();
    let caps = capricorn_types();
    let others = other_range_two_mover_types();
    if hooks.is_empty() {
        return Err("HookMover class empty".into());
    }
    if caps.is_empty() {
        return Err("Capricorn class empty".into());
    }
    if others.is_empty() {
        return Err("no other range two-movers found".into());
    }
    if !others.contains(&PieceType::Tengu) || !others.contains(&PieceType::Peacock) {
        return Err("expected Tengu/Peacock in other range two-mover class".into());
    }
    if others.contains(&PieceType::HookMover) || others.contains(&PieceType::Capricorn) {
        return Err("HookMover/Capricorn must not be in the other two-mover class".into());
    }

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(27);
    let mut entrants = Vec::with_capacity(27);
    for &hm in &HOOK_MOVER_MULTS {
        for &cm in &CAPRICORN_MULTS {
            for &om in &OTHER_TWO_MOVER_MULTS {
                let id = cell_id(pct(hm), pct(cm), pct(om));
                let model_path = cfg.out_dir.join(format!("{id}.json"));
                let is_center = (hm - 1.0).abs() < 1e-6
                    && (cm - 1.0).abs() < 1e-6
                    && (om - 1.0).abs() < 1e-6;
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
                    cp.weights = apply_hco_mults(base, hm, cm, om);
                    cp.save_path(&model_path)
                        .map_err(|e| format!("save {}: {e}", model_path.display()))?;
                }
                let model = model_path.display().to_string();
                cells.push(GridCell {
                    id: id.clone(),
                    model: model.clone(),
                    hook_mover_mult: hm,
                    capricorn_mult: cm,
                    other_two_mover_mult: om,
                });
                entrants.push(TourneyEntrant { id, model });
            }
        }
    }

    let grid = GridFile {
        seed_model: cfg.seed_model.display().to_string(),
        hook_mover_pieces: hooks.iter().map(|pt| format!("{pt:?}")).collect(),
        capricorn_pieces: caps.iter().map(|pt| format!("{pt:?}")).collect(),
        other_two_mover_pieces: others.iter().map(|pt| format!("{pt:?}")).collect(),
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
        let mid = apply_hco_mults(&seed, 1.0, 1.0, 1.0);
        for pt in hook_mover_types()
            .into_iter()
            .chain(capricorn_types())
            .chain(other_range_two_mover_types())
        {
            assert!((mid.piece_value(pt) - seed.piece_value(pt)).abs() < 1e-3);
        }
    }

    #[test]
    fn corner_scales_axes_independently() {
        let seed = EvalWeights::seed();
        let w = apply_hco_mults(&seed, 1.1, 0.8, 1.1);
        assert!(
            (w.piece_value(PieceType::HookMover) - seed.piece_value(PieceType::HookMover) * 1.1)
                .abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Capricorn) - seed.piece_value(PieceType::Capricorn) * 0.8)
                .abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Tengu) - seed.piece_value(PieceType::Tengu) * 1.1).abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Peacock) - seed.piece_value(PieceType::Peacock) * 1.1).abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::GreatGeneral)
                - seed.piece_value(PieceType::GreatGeneral))
                .abs()
                < 1e-3
        );
    }

    #[test]
    fn class_partitions() {
        let others = other_range_two_mover_types();
        assert!(!others.contains(&PieceType::HookMover));
        assert!(!others.contains(&PieceType::Capricorn));
        assert!(others.contains(&PieceType::Tengu));
        assert!(others.contains(&PieceType::Peacock));
    }

    #[test]
    fn cell_ids_cover_27() {
        let mut ids = Vec::new();
        for &hm in &HOOK_MOVER_MULTS {
            for &cm in &CAPRICORN_MULTS {
                for &om in &OTHER_TWO_MOVER_MULTS {
                    ids.push(cell_id(pct(hm), pct(cm), pct(om)));
                }
            }
        }
        assert_eq!(ids.len(), 27);
        assert!(ids.contains(&"H100C100O100".to_string()));
        assert!(ids.contains(&"H90C80O80".to_string()));
        assert!(ids.contains(&"H110C120O110".to_string()));
        assert_eq!(pct(0.90), 90);
        assert_eq!(pct(1.10), 110);
        assert_eq!(pct(1.20), 120);
    }
}
