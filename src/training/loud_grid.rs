//! 3×3 material grid over HookMover × other range two-movers.
//!
//! Default grid (percent of seed): HookMover H ∈ {80, 100, 120}, other range
//! two-movers O ∈ {80, 100, 120}. Capturers stay at seed. Center cell `H100O100`
//! is a byte-copy of the seed checkpoint.

use crate::eval::{is_range_two_mover, EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::piece::PieceType;
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/loud-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// HookMover multipliers (H80 / H100 / H120).
pub const HOOK_MOVER_MULTS: [f32; 3] = [0.80, 1.0, 1.20];
/// Other range two-mover multipliers (O80 / O100 / O120): Tengu, Peacock, Capricorn, …
pub const OTHER_TWO_MOVER_MULTS: [f32; 3] = [0.80, 1.0, 1.20];

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
    pub other_two_mover_mult: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub hook_mover_pieces: Vec<String>,
    pub other_two_mover_pieces: Vec<String>,
    pub cells: Vec<GridCell>,
}

pub fn hook_mover_types() -> Vec<PieceType> {
    vec![PieceType::HookMover]
}

/// Range two-movers excluding HookMover (Tengu / Peacock / Capricorn / …).
pub fn other_range_two_mover_types() -> Vec<PieceType> {
    ALL_PIECE_TYPES
        .iter()
        .copied()
        .filter(|pt| is_range_two_mover(*pt) && *pt != PieceType::HookMover)
        .collect()
}

pub fn cell_id(hook_pct: u32, other_pct: u32) -> String {
    format!("H{hook_pct}O{other_pct}")
}

fn pct(mult: f32) -> u32 {
    (mult * 100.0).round() as u32
}

/// Scale HookMover and the other range-two-mover bloc independently on a clone of `base`.
pub fn apply_hook_other_mults(
    base: &EvalWeights,
    hook_mover_mult: f32,
    other_two_mover_mult: f32,
) -> EvalWeights {
    let mut w = base.clone();
    for pt in hook_mover_types() {
        let v = base.piece_value(pt) * hook_mover_mult;
        w.piece.insert(pt, v);
    }
    for pt in other_range_two_mover_types() {
        let v = base.piece_value(pt) * other_two_mover_mult;
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
    let hooks = hook_mover_types();
    let others = other_range_two_mover_types();
    if hooks.is_empty() {
        return Err("HookMover class empty".into());
    }
    if others.is_empty() {
        return Err("no other range two-movers found".into());
    }
    if !others.contains(&PieceType::Tengu)
        || !others.contains(&PieceType::Peacock)
        || !others.contains(&PieceType::Capricorn)
    {
        return Err("expected Tengu/Peacock/Capricorn in other range two-mover class".into());
    }
    if others.contains(&PieceType::HookMover) {
        return Err("HookMover must not be in the other two-mover class".into());
    }

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(9);
    let mut entrants = Vec::with_capacity(9);
    for &hm in &HOOK_MOVER_MULTS {
        for &om in &OTHER_TWO_MOVER_MULTS {
            let id = cell_id(pct(hm), pct(om));
            let model_path = cfg.out_dir.join(format!("{id}.json"));
            if (hm - 1.0).abs() < 1e-6 && (om - 1.0).abs() < 1e-6 {
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
                cp.weights = apply_hook_other_mults(base, hm, om);
                cp.save_path(&model_path)
                    .map_err(|e| format!("save {}: {e}", model_path.display()))?;
            }
            let model = model_path.display().to_string();
            cells.push(GridCell {
                id: id.clone(),
                model: model.clone(),
                hook_mover_mult: hm,
                other_two_mover_mult: om,
            });
            entrants.push(TourneyEntrant { id, model });
        }
    }

    let grid = GridFile {
        seed_model: cfg.seed_model.display().to_string(),
        hook_mover_pieces: hooks.iter().map(|pt| format!("{pt:?}")).collect(),
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
        let mid = apply_hook_other_mults(&seed, 1.0, 1.0);
        for pt in hook_mover_types() {
            assert!((mid.piece_value(pt) - seed.piece_value(pt)).abs() < 1e-3);
        }
        for pt in other_range_two_mover_types() {
            assert!((mid.piece_value(pt) - seed.piece_value(pt)).abs() < 1e-3);
        }
    }

    #[test]
    fn corner_scales_hook_and_others_independently() {
        let seed = EvalWeights::seed();
        let w = apply_hook_other_mults(&seed, 1.2, 0.8);
        assert!(
            (w.piece_value(PieceType::HookMover) - seed.piece_value(PieceType::HookMover) * 1.2)
                .abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Tengu) - seed.piece_value(PieceType::Tengu) * 0.8).abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Peacock) - seed.piece_value(PieceType::Peacock) * 0.8).abs()
                < 1e-2
        );
        assert!(
            (w.piece_value(PieceType::Capricorn) - seed.piece_value(PieceType::Capricorn) * 0.8)
                .abs()
                < 1e-2
        );
        // Capturers / lion untouched.
        assert!(
            (w.piece_value(PieceType::GreatGeneral)
                - seed.piece_value(PieceType::GreatGeneral))
                .abs()
                < 1e-3
        );
        assert!((w.piece_value(PieceType::FreeKing) - seed.piece_value(PieceType::FreeKing)).abs() < 1e-3);
        assert!((w.piece_value(PieceType::Lion) - seed.piece_value(PieceType::Lion)).abs() < 1e-3);
    }

    #[test]
    fn other_class_excludes_hook() {
        let others = other_range_two_mover_types();
        assert!(!others.contains(&PieceType::HookMover));
        assert!(others.contains(&PieceType::Tengu));
        assert!(others.contains(&PieceType::Peacock));
        assert!(others.contains(&PieceType::Capricorn));
    }

    #[test]
    fn cell_ids_cover_grid() {
        let mut ids = Vec::new();
        for &hm in &HOOK_MOVER_MULTS {
            for &om in &OTHER_TWO_MOVER_MULTS {
                ids.push(cell_id(pct(hm), pct(om)));
            }
        }
        assert_eq!(ids.len(), 9);
        assert!(ids.contains(&"H100O100".to_string()));
        assert!(ids.contains(&"H80O80".to_string()));
        assert!(ids.contains(&"H120O120".to_string()));
        assert!(ids.contains(&"H120O80".to_string()));
        assert_eq!(pct(0.80), 80);
        assert_eq!(pct(1.20), 120);
    }
}
