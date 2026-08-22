//! Finalist two-mover mobility cells (A1, C1/C2, K100/K200) plus a q-blowup twin
//! of each, SEED, and history baselines.
//!
//! Q twins share weights with the current-search cell and set
//! `sibling_mode=2` (same-wipe LMR R=2) + `q_loud_promo_simple_only`.

use crate::eval::{EvalCheckpoint, SearchDefaults};
use crate::training::history::{append_history_entrants, DEFAULT_MANIFEST};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use crate::training::two_mob_grid::{apply_two_mob_cell, cell_id};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/two-mob-q-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// (curve, k, apply) finalists from the 18-cell two-mob Swiss.
pub const FINALISTS: [(u8, f32, u8); 4] = [
    (1, 100.0, 1),
    (1, 200.0, 1),
    (2, 100.0, 1),
    (2, 200.0, 1),
];

/// Same-wipe LMR R=2 (see `SearchConfig::sibling_mode`).
pub const Q_BLOWUP_SIBLING_MODE: u8 = 2;

#[derive(Debug, Clone)]
pub struct TwoMobQGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for TwoMobQGridConfig {
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
    pub q_blowup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn q_cell_id(curve: u8, k: f32, apply: u8) -> String {
    format!("{}Q", cell_id(curve, k, apply))
}

pub fn q_blowup_search_defaults(base: &SearchDefaults) -> SearchDefaults {
    let mut s = base.clone();
    s.sibling_mode = Q_BLOWUP_SIBLING_MODE;
    s.q_loud_promo_simple_only = true;
    s
}

fn push_cell(
    cells: &mut Vec<GridCell>,
    entrants: &mut Vec<TourneyEntrant>,
    id: String,
    model: String,
    curve: u8,
    k: f32,
    apply: u8,
    q_blowup: bool,
) {
    cells.push(GridCell {
        id: id.clone(),
        model: model.clone(),
        two_mover_mob_k: k,
        two_mover_mob_curve: curve,
        two_mover_mob_apply: apply,
        q_blowup,
    });
    entrants.push(TourneyEntrant {
        id,
        model,
        engine: None,
    });
}

pub fn run_two_mob_q_grid(cfg: &TwoMobQGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    let mut cells = Vec::with_capacity(9);
    let mut entrants = Vec::with_capacity(19);

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
        q_blowup: false,
    });
    entrants.push(TourneyEntrant {
        id: seed_id.into(),
        model: seed_path.display().to_string(),
        engine: None,
    });

    for &(curve, k, apply) in &FINALISTS {
        let id = cell_id(curve, k, apply);
        let model_path = cfg.out_dir.join(format!("{id}.json"));
        let mut cp = base_cp.clone();
        cp.name = id.clone();
        cp.weights = apply_two_mob_cell(base, curve, k, apply);
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        push_cell(
            &mut cells,
            &mut entrants,
            id,
            model_path.display().to_string(),
            curve,
            k,
            apply,
            false,
        );

        let qid = q_cell_id(curve, k, apply);
        let q_path = cfg.out_dir.join(format!("{qid}.json"));
        let mut qcp = base_cp.clone();
        qcp.name = qid.clone();
        qcp.weights = apply_two_mob_cell(base, curve, k, apply);
        qcp.search_defaults = q_blowup_search_defaults(&base_cp.search_defaults);
        qcp.save_path(&q_path)
            .map_err(|e| format!("save {}: {e}", q_path.display()))?;
        push_cell(
            &mut cells,
            &mut entrants,
            qid,
            q_path.display().to_string(),
            curve,
            k,
            apply,
            true,
        );
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
    fn finalist_ids_and_q_twins() {
        let ids: Vec<String> = FINALISTS
            .iter()
            .flat_map(|&(c, k, a)| [cell_id(c, k, a), q_cell_id(c, k, a)])
            .collect();
        assert_eq!(
            ids,
            vec![
                "C1K100A1",
                "C1K100A1Q",
                "C1K200A1",
                "C1K200A1Q",
                "C2K100A1",
                "C2K100A1Q",
                "C2K200A1",
                "C2K200A1Q",
            ]
        );
    }

    #[test]
    fn q_defaults_set_lmr2_and_loud_st() {
        let s = q_blowup_search_defaults(&SearchDefaults::default());
        assert_eq!(s.sibling_mode, 2);
        assert!(s.q_loud_promo_simple_only);
        assert_eq!(s.quiescence_depth, 2);
    }

    #[test]
    fn apply_matches_two_mob_cell() {
        let seed = EvalWeights::seed();
        let w = apply_two_mob_cell(&seed, 1, 200.0, 1);
        assert!((w.two_mover_mob_k - 200.0).abs() < 1e-6);
        assert_eq!(w.two_mover_mob_curve, 1);
        assert_eq!(w.two_mover_mob_apply, 1);
    }

    #[test]
    fn grid_pairs_finalists_and_keeps_history() {
        let tmp = std::env::temp_dir().join(format!("tk-two-mob-q-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let cfg = TwoMobQGridConfig {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        if !cfg.seed_model.is_file() {
            EvalCheckpoint::seed("ab-seed")
                .save_path(&cfg.seed_model)
                .unwrap();
        }
        let (man, grid) = run_two_mob_q_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 9);
        assert!(man.entrants.iter().any(|e| e.id == "SEED"));
        assert!(man.entrants.iter().any(|e| e.id == "C1K100A1"));
        assert!(man.entrants.iter().any(|e| e.id == "C1K100A1Q"));
        assert!(man.entrants.iter().any(|e| e.id == "C2K200A1Q"));
        assert!(!man.entrants.iter().any(|e| e.id == "C0K100A1"));
        assert!(!man.entrants.iter().any(|e| e.id == "C1K40A1"));
        assert!(man.entrants.iter().any(|e| e.id == "BASE_P120H75B60"));
        let logic = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_H105")
            .expect("LOGIC_H105");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_H105"));
        assert_eq!(man.entrants.len(), 21);

        let q = EvalCheckpoint::load_path(tmp.join("C2K100A1Q.json")).unwrap();
        assert_eq!(q.search_defaults.sibling_mode, 2);
        assert!(q.search_defaults.q_loud_promo_simple_only);
        let plain = EvalCheckpoint::load_path(tmp.join("C2K100A1.json")).unwrap();
        assert_eq!(plain.search_defaults.sibling_mode, 0);
        assert!(!plain.search_defaults.q_loud_promo_simple_only);
        assert_eq!(q.weights.two_mover_mob_curve, plain.weights.two_mover_mob_curve);
        assert!((q.weights.two_mover_mob_k - plain.weights.two_mover_mob_k).abs() < 1e-6);

        let _ = fs::remove_dir_all(&tmp);
    }
}
