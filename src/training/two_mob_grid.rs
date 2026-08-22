//! 3×3×2 two-mover mobility grid (18 cells) plus SEED and history baselines.
//!
//! File apply (A2) is omitted: the current seed's `file_factor` is flat 1.0, so
//! A2 was a no-op vs A0.

use crate::eval::{EvalCheckpoint, EvalWeights};
use crate::training::history::{append_history_entrants, DEFAULT_MANIFEST};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/two-mob-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

pub const CURVES: [u8; 3] = [0, 1, 2];
pub const KS: [f32; 3] = [40.0, 100.0, 200.0];
pub const APPLIES: [u8; 2] = [0, 1];

#[derive(Debug, Clone)]
pub struct TwoMobGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for TwoMobGridConfig {
    fn default() -> Self {
        Self {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridCell {
    pub id: String,
    pub model: String,
    pub two_mover_mob_k: f32,
    pub two_mover_mob_curve: u8,
    pub two_mover_mob_apply: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn cell_id(curve: u8, k: f32, apply: u8) -> String {
    format!("C{curve}K{}A{apply}", k.round() as u32)
}

pub fn apply_two_mob_cell(base: &EvalWeights, curve: u8, k: f32, apply: u8) -> EvalWeights {
    let mut w = base.clone();
    w.two_mover_mob_k = k;
    w.two_mover_mob_curve = curve;
    w.two_mover_mob_apply = apply;
    w
}

pub fn run_two_mob_grid(cfg: &TwoMobGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    let mut cells = Vec::with_capacity(19);
    let mut entrants = Vec::with_capacity(29);

    let seed_id = "SEED";
    let seed_path = cfg.out_dir.join("SEED.json");
    fs::copy(&cfg.seed_model, &seed_path).map_err(|e| {
        format!(
            "copy {} → {}: {e}",
            cfg.seed_model.display(),
            seed_path.display()
        )
    })?;
    cells.push(GridCell {
        id: seed_id.into(),
        model: seed_path.display().to_string(),
        two_mover_mob_k: 0.0,
        two_mover_mob_curve: 0,
        two_mover_mob_apply: 0,
    });
    entrants.push(TourneyEntrant {
        id: seed_id.into(),
        model: seed_path.display().to_string(),
        engine: None,
    });

    for &curve in &CURVES {
        for &k in &KS {
            for &apply in &APPLIES {
                let id = cell_id(curve, k, apply);
                let model_path = cfg.out_dir.join(format!("{id}.json"));
                let mut cp = base_cp.clone();
                cp.name = id.clone();
                cp.weights = apply_two_mob_cell(base, curve, k, apply);
                cp.save_path(&model_path)
                    .map_err(|e| format!("save {}: {e}", model_path.display()))?;
                let model = model_path.display().to_string();
                cells.push(GridCell {
                    id: id.clone(),
                    model: model.clone(),
                    two_mover_mob_k: k,
                    two_mover_mob_curve: curve,
                    two_mover_mob_apply: apply,
                });
                entrants.push(TourneyEntrant {
                    id,
                    model,
                    engine: None,
                });
            }
        }
    }

    append_history_entrants(&cfg.history_manifest, &cfg.out_dir, &mut entrants)?;

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
    fn cell_ids_cover_18_plus_seed() {
        let mut ids = vec!["SEED".to_string()];
        for &c in &CURVES {
            for &k in &KS {
                for &a in &APPLIES {
                    ids.push(cell_id(c, k, a));
                }
            }
        }
        assert_eq!(ids.len(), 19);
        assert!(ids.contains(&"C0K100A0".to_string()));
        assert!(ids.contains(&"C2K200A1".to_string()));
        assert!(ids.contains(&"C1K40A1".to_string()));
        assert!(!ids.iter().any(|id| id.ends_with("A2")));
    }

    #[test]
    fn apply_sets_mobility_knobs() {
        let seed = EvalWeights::seed();
        assert!((seed.two_mover_mob_k - 0.0).abs() < 1e-6);
        let w = apply_two_mob_cell(&seed, 2, 100.0, 1);
        assert!((w.two_mover_mob_k - 100.0).abs() < 1e-6);
        assert_eq!(w.two_mover_mob_curve, 2);
        assert_eq!(w.two_mover_mob_apply, 1);
    }

    #[test]
    fn grid_writes_seed_base_and_logic_engine_fields() {
        let tmp = std::env::temp_dir().join(format!("tk-two-mob-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let cfg = TwoMobGridConfig {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        if !cfg.seed_model.is_file() {
            EvalCheckpoint::seed("ab-seed")
                .save_path(&cfg.seed_model)
                .unwrap();
        }
        let (man, grid) = run_two_mob_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 19);
        assert!(man.entrants.iter().any(|e| e.id == "SEED"));
        assert!(man.entrants.iter().any(|e| e.id == "C0K100A0"));
        assert!(man.entrants.iter().any(|e| e.id == "BASE_P120H75B60"));
        assert!(man.entrants.iter().any(|e| e.id == "BASE_H105O105"));
        assert!(!man.entrants.iter().any(|e| e.id.ends_with("A2")));
        let logic = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_H105")
            .expect("LOGIC_H105");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_H105"));
        assert!(man.entrants.iter().any(|e| e.id == "LOGIC_B65T12"));
        assert_eq!(man.entrants.len(), 31);
        let _ = fs::remove_dir_all(&tmp);
    }
}
