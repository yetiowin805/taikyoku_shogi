//! Mix-tournament top 11 + C2K50A1 twins + leftover playable history.
//!
//! Field (27):
//! - Top 11 from `top4-mix-swiss-20260820`
//! - C2K50A1 mobility twins of those chassis except `C2K50A1` and `SEED`
//!   (`C2K100A1D50` keeps D50 → `C2K50A1D50`)
//! - Leftover history: PRELOUD, T150C50, T150C120, H105O105, P120H75B60,
//!   LOGIC_H105, LOGIC_HANGQ_ANY
//!   (skip TROPISM, B65T12, HANGQ_ST, HANGQ_AB, and top-11 overlaps)

use crate::eval::{EvalCheckpoint, EvalWeights};
use crate::piece::PieceType;
use crate::training::history::{
    append_history_entrants_except, load_seed_at, HistoryManifest, DEFAULT_MANIFEST,
};
use crate::training::top4_mix_grid::{
    apply_cell, apply_two_mob_extra, mix_weights, MATERIALS, PSTS,
};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/top11-c2-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// Mix-tournament top 11 (same order as [`deploy/compare_top11_texel.py`]).
pub const TOP11: [&str; 11] = [
    "T150_P120_T12",
    "H120_P120_T15",
    "AVG_T150_H120",
    "C2K50A1",
    "BASE_P120H50B75",
    "BASE_H120O80",
    "SEED",
    "H120_B65_T12",
    "AVG_P120_SEED",
    "T150_B65_T12",
    "C2K100A1D50",
];

/// Already have C2K50A1 mobility (SEED is the same chassis without the bonus).
pub const NO_C2_TWIN: &[&str] = &["C2K50A1", "SEED"];

/// History already in the top 11, plus engines we do not play.
pub const SKIP_HISTORY: &[&str] = &[
    "BASE_H120O80",
    "BASE_P120H50B75",
    "LOGIC_TROPISM",
    "LOGIC_B65T12",
    "LOGIC_HANGQ_ST",
    "LOGIC_HANGQ_AB",
];

#[derive(Debug, Clone)]
pub struct Top11C2GridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for Top11C2GridConfig {
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
    pub kind: String,
    pub parent_id: Option<String>,
    pub two_mover_mob_k: Option<f32>,
    pub two_mover_discount: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn c2_twin_id(base_id: &str) -> Option<String> {
    if NO_C2_TWIN.contains(&base_id) {
        None
    } else if base_id == "C2K100A1D50" {
        Some("C2K50A1D50".into())
    } else {
        Some(format!("{base_id}_C2"))
    }
}

fn push_entrant(entrants: &mut Vec<TourneyEntrant>, id: String, model: String) {
    entrants.push(TourneyEntrant {
        id,
        model,
        engine: None,
    });
}

fn piece_map<'a>(
    maps: &'a HashMap<&str, HashMap<PieceType, f32>>,
    tag: &str,
) -> Result<&'a HashMap<PieceType, f32>, String> {
    maps.get(tag)
        .ok_or_else(|| format!("missing {tag} piece map"))
}

/// Rebuild the mix-tournament top 11 from material maps + seed extras.
pub fn build_top11_weights(
    base: &EvalWeights,
    t150: &HashMap<PieceType, f32>,
    h120: &HashMap<PieceType, f32>,
    h105: &HashMap<PieceType, f32>,
) -> Vec<(String, EvalWeights)> {
    let old = &PSTS[0];
    let p120 = &PSTS[1];
    let b65 = &PSTS[2];

    let base_p120 = apply_cell(base, h105, p120, 1.5);
    let seed = apply_cell(base, h105, b65, 1.2);

    vec![
        ("T150_P120_T12".into(), apply_cell(base, t150, p120, 1.2)),
        ("H120_P120_T15".into(), apply_cell(base, h120, p120, 1.5)),
        (
            "AVG_T150_H120".into(),
            mix_weights(
                &apply_cell(base, t150, old, 1.5),
                &apply_cell(base, h120, old, 1.5),
            ),
        ),
        ("C2K50A1".into(), apply_two_mob_extra(base, 50.0, 0.0)),
        ("BASE_P120H50B75".into(), base_p120.clone()),
        ("BASE_H120O80".into(), apply_cell(base, h120, old, 1.5)),
        ("SEED".into(), seed.clone()),
        ("H120_B65_T12".into(), apply_cell(base, h120, b65, 1.2)),
        ("AVG_P120_SEED".into(), mix_weights(&base_p120, &seed)),
        ("T150_B65_T12".into(), apply_cell(base, t150, b65, 1.2)),
        ("C2K100A1D50".into(), apply_two_mob_extra(base, 100.0, 50.0)),
    ]
}

fn twin_weights(parent_id: &str, parent: &EvalWeights, seed: &EvalWeights) -> EvalWeights {
    if parent_id == "C2K100A1D50" {
        apply_two_mob_extra(seed, 50.0, 50.0)
    } else {
        apply_two_mob_extra(parent, 50.0, 0.0)
    }
}

/// Write top 11 + C2 twins + leftover history and a knockout manifest.
pub fn run_top11_c2_grid(cfg: &Top11C2GridConfig) -> Result<(TourneyManifest, GridFile), String> {
    if !cfg.seed_model.is_file() {
        return Err(format!(
            "missing seed model {} (export-seed or copy checkpoint first)",
            cfg.seed_model.display()
        ));
    }
    let base_cp = EvalCheckpoint::load_path(&cfg.seed_model)?;
    let base = &base_cp.weights;
    let hist = HistoryManifest::load_path(&cfg.history_manifest)?;

    let mut piece_maps: HashMap<&str, HashMap<PieceType, f32>> = HashMap::new();
    for mat in &MATERIALS {
        let rev = hist.git_for(mat.history_id)?;
        let cp = load_seed_at(rev, mat.history_id)?;
        piece_maps.insert(mat.tag, cp.weights.piece);
    }
    let t150 = piece_map(&piece_maps, "T150")?;
    let h120 = piece_map(&piece_maps, "H120")?;
    let h105 = piece_map(&piece_maps, "H105")?;

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let top11 = build_top11_weights(base, t150, h120, h105);
    debug_assert_eq!(
        top11.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
        TOP11
    );

    let mut cells = Vec::with_capacity(20);
    let mut entrants = Vec::with_capacity(27);
    let mut by_id: HashMap<String, EvalWeights> = HashMap::new();

    for (id, weights) in top11 {
        let model_path = cfg.out_dir.join(format!("{id}.json"));
        let mut cp = base_cp.clone();
        cp.name = id.clone();
        cp.weights = weights.clone();
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        by_id.insert(id.clone(), weights);
        let model = model_path.display().to_string();
        cells.push(GridCell {
            id: id.clone(),
            model: model.clone(),
            kind: "top11".into(),
            parent_id: None,
            two_mover_mob_k: None,
            two_mover_discount: None,
        });
        push_entrant(&mut entrants, id, model);
    }

    let seed_w = by_id
        .get("SEED")
        .ok_or_else(|| "missing SEED weights".to_string())?;
    for &parent_id in &TOP11 {
        let Some(twin_id) = c2_twin_id(parent_id) else {
            continue;
        };
        let parent = by_id
            .get(parent_id)
            .ok_or_else(|| format!("missing parent {parent_id}"))?;
        let weights = twin_weights(parent_id, parent, seed_w);
        let model_path = cfg.out_dir.join(format!("{twin_id}.json"));
        let mut cp = base_cp.clone();
        cp.name = twin_id.clone();
        cp.weights = weights;
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        let discount = if parent_id == "C2K100A1D50" {
            Some(50.0)
        } else {
            Some(0.0)
        };
        let model = model_path.display().to_string();
        cells.push(GridCell {
            id: twin_id.clone(),
            model: model.clone(),
            kind: "c2_twin".into(),
            parent_id: Some(parent_id.into()),
            two_mover_mob_k: Some(50.0),
            two_mover_discount: discount,
        });
        push_entrant(&mut entrants, twin_id, model);
    }

    append_history_entrants_except(
        &cfg.history_manifest,
        &cfg.out_dir,
        &mut entrants,
        SKIP_HISTORY,
    )?;

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
    use crate::piece::PieceType;

    #[test]
    fn twin_ids_skip_c2_and_seed() {
        assert_eq!(
            c2_twin_id("T150_P120_T12").as_deref(),
            Some("T150_P120_T12_C2")
        );
        assert_eq!(
            c2_twin_id("BASE_P120H50B75").as_deref(),
            Some("BASE_P120H50B75_C2")
        );
        assert_eq!(c2_twin_id("C2K100A1D50").as_deref(), Some("C2K50A1D50"));
        assert_eq!(c2_twin_id("C2K50A1"), None);
        assert_eq!(c2_twin_id("SEED"), None);
        let twins: Vec<_> = TOP11.iter().filter_map(|id| c2_twin_id(id)).collect();
        assert_eq!(twins.len(), 9);
        let uniq: HashSet<_> = twins.iter().cloned().collect();
        assert_eq!(uniq.len(), 9);
    }

    #[test]
    fn grid_writes_27_unique_entrants() {
        let tmp = std::env::temp_dir().join(format!("tk-top11-c2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let seed_path = if PathBuf::from(DEFAULT_SEED_MODEL).is_file() {
            PathBuf::from(DEFAULT_SEED_MODEL)
        } else {
            let p = tmp.join("ab-seed.json");
            EvalCheckpoint::seed("ab-seed").save_path(&p).unwrap();
            p
        };
        let cfg = Top11C2GridConfig {
            seed_model: seed_path,
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        let (man, grid) = run_top11_c2_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.iter().filter(|c| c.kind == "top11").count(), 11);
        assert_eq!(grid.cells.iter().filter(|c| c.kind == "c2_twin").count(), 9);
        let ids: Vec<_> = man.entrants.iter().map(|e| e.id.as_str()).collect();
        let uniq: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(uniq.len(), ids.len(), "duplicate ids: {ids:?}");
        assert_eq!(man.entrants.len(), 27);
        for id in TOP11 {
            assert!(ids.contains(&id), "missing {id}");
        }
        for id in [
            "T150_P120_T12_C2",
            "H120_P120_T15_C2",
            "AVG_T150_H120_C2",
            "BASE_P120H50B75_C2",
            "BASE_H120O80_C2",
            "H120_B65_T12_C2",
            "AVG_P120_SEED_C2",
            "T150_B65_T12_C2",
            "C2K50A1D50",
        ] {
            assert!(ids.contains(&id), "missing twin {id}");
        }
        assert!(!ids.contains(&"C2K50A1_C2"));
        assert!(!ids.contains(&"SEED_C2"));
        for id in [
            "BASE_PRELOUD",
            "BASE_T150C50",
            "BASE_T150C120",
            "BASE_H105O105",
            "BASE_P120H75B60",
            "LOGIC_H105",
            "LOGIC_HANGQ_ANY",
        ] {
            assert!(ids.contains(&id), "missing leftover {id}");
        }
        for id in [
            "LOGIC_TROPISM",
            "LOGIC_B65T12",
            "LOGIC_HANGQ_ST",
            "LOGIC_HANGQ_AB",
        ] {
            assert!(!ids.contains(&id), "unexpected {id}");
        }

        let seed = EvalCheckpoint::load_path(&cfg.seed_model).unwrap();
        let t150 = EvalCheckpoint::load_path(tmp.join("T150_P120_T12.json")).unwrap();
        let t150_c2 = EvalCheckpoint::load_path(tmp.join("T150_P120_T12_C2.json")).unwrap();
        assert!((t150.weights.two_mover_mob_k - 0.0).abs() < 1e-6);
        assert!((t150_c2.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert_eq!(t150_c2.weights.two_mover_mob_curve, 2);
        assert_eq!(t150_c2.weights.two_mover_mob_apply, 1);
        assert_eq!(
            t150_c2.weights.piece[&PieceType::HookMover],
            t150.weights.piece[&PieceType::HookMover]
        );

        let c2 = EvalCheckpoint::load_path(tmp.join("C2K50A1.json")).unwrap();
        assert!((c2.weights.two_mover_mob_k - 50.0).abs() < 1e-6);

        let d50 = EvalCheckpoint::load_path(tmp.join("C2K100A1D50.json")).unwrap();
        let d50_c2 = EvalCheckpoint::load_path(tmp.join("C2K50A1D50.json")).unwrap();
        assert!((d50.weights.two_mover_mob_k - 100.0).abs() < 1e-6);
        assert!((d50_c2.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert!(
            (d50_c2.weights.piece[&PieceType::HookMover]
                - (seed.weights.piece[&PieceType::HookMover] - 50.0))
                .abs()
                < 1e-3
        );
        assert_eq!(
            d50_c2.weights.piece[&PieceType::HookMover],
            d50.weights.piece[&PieceType::HookMover]
        );

        let logic = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_H105")
            .expect("LOGIC_H105");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_H105"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
