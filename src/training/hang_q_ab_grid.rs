//! Mini hang-q A/B grid: four mix-tournament weights × current / A / B / AB.
//!
//! Bases (top of `top4-mix-swiss-20260820`):
//! - `T150_P120_T12`, `H120_P120_T15`, `AVG_T150_H120`, `C2K50A1`
//!
//! Search twins share weights and set `hang_q_dest_multileg` / `hang_q_dest_pathclear`.
//! 16 agents (4 × 4), no history leftovers.

use crate::eval::{EvalCheckpoint, SearchDefaults};
use crate::piece::PieceType;
use crate::training::history::{load_seed_at, HistoryManifest, DEFAULT_MANIFEST};
use crate::training::top4_mix_grid::{
    apply_cell, apply_two_mob_extra, mix_weights, MATERIALS, PSTS,
};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/hang-q-ab-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// (id, hang A, hang B)
pub const SEARCH_VARIANTS: [(&str, bool, bool); 4] = [
    ("", false, false),
    ("A", true, false),
    ("B", false, true),
    ("AB", true, true),
];

#[derive(Debug, Clone)]
pub struct HangQAbGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for HangQAbGridConfig {
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
    pub base_id: String,
    pub hang_q_dest_multileg: bool,
    pub hang_q_dest_pathclear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn variant_id(base_id: &str, suffix: &str) -> String {
    if suffix.is_empty() {
        base_id.to_string()
    } else {
        format!("{base_id}{suffix}")
    }
}

pub fn apply_hang_q_search(base: &SearchDefaults, a: bool, b: bool) -> SearchDefaults {
    let mut s = base.clone();
    s.hang_q_dest_multileg = a;
    s.hang_q_dest_pathclear = b;
    s
}

fn push_variants(
    cells: &mut Vec<GridCell>,
    entrants: &mut Vec<TourneyEntrant>,
    base_id: &str,
    out_dir: &std::path::Path,
    mut cp: EvalCheckpoint,
) -> Result<(), String> {
    let search0 = cp.search_defaults.clone();
    let weights = cp.weights.clone();
    for &(suffix, a, b) in &SEARCH_VARIANTS {
        let id = variant_id(base_id, suffix);
        let model_path = out_dir.join(format!("{id}.json"));
        cp.name = id.clone();
        cp.weights = weights.clone();
        cp.search_defaults = apply_hang_q_search(&search0, a, b);
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        let model = model_path.display().to_string();
        cells.push(GridCell {
            id: id.clone(),
            model: model.clone(),
            base_id: base_id.into(),
            hang_q_dest_multileg: a,
            hang_q_dest_pathclear: b,
        });
        entrants.push(TourneyEntrant {
            id,
            model,
            engine: None,
        });
    }
    Ok(())
}

/// Write 16 checkpoints (4 weights × current/A/B/AB) and a knockout manifest.
pub fn run_hang_q_ab_grid(cfg: &HangQAbGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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
    let t150 = piece_maps
        .get("T150")
        .ok_or_else(|| "missing T150 piece map".to_string())?;
    let h120 = piece_maps
        .get("H120")
        .ok_or_else(|| "missing H120 piece map".to_string())?;

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(16);
    let mut entrants = Vec::with_capacity(16);

    let mut t150_p120 = base_cp.clone();
    t150_p120.weights = apply_cell(base, t150, &PSTS[1], 1.2);
    push_variants(
        &mut cells,
        &mut entrants,
        "T150_P120_T12",
        &cfg.out_dir,
        t150_p120,
    )?;

    let mut h120_p120 = base_cp.clone();
    h120_p120.weights = apply_cell(base, h120, &PSTS[1], 1.5);
    push_variants(
        &mut cells,
        &mut entrants,
        "H120_P120_T15",
        &cfg.out_dir,
        h120_p120,
    )?;

    let t150_old = apply_cell(base, t150, &PSTS[0], 1.5);
    let h120_old = apply_cell(base, h120, &PSTS[0], 1.5);
    let mut avg = base_cp.clone();
    avg.weights = mix_weights(&t150_old, &h120_old);
    push_variants(
        &mut cells,
        &mut entrants,
        "AVG_T150_H120",
        &cfg.out_dir,
        avg,
    )?;

    let mut c2 = base_cp.clone();
    c2.weights = apply_two_mob_extra(base, 50.0, 0.0);
    push_variants(&mut cells, &mut entrants, "C2K50A1", &cfg.out_dir, c2)?;

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
    fn variant_ids() {
        assert_eq!(variant_id("C2K50A1", ""), "C2K50A1");
        assert_eq!(variant_id("C2K50A1", "A"), "C2K50A1A");
        assert_eq!(variant_id("T150_P120_T12", "AB"), "T150_P120_T12AB");
    }

    #[test]
    fn search_defaults_set_only_requested_flags() {
        let a = apply_hang_q_search(&SearchDefaults::default(), true, false);
        assert!(a.hang_q_dest_multileg);
        assert!(!a.hang_q_dest_pathclear);
        let ab = apply_hang_q_search(&SearchDefaults::default(), true, true);
        assert!(ab.hang_q_dest_multileg && ab.hang_q_dest_pathclear);
        let cur = apply_hang_q_search(&SearchDefaults::default(), false, false);
        assert!(!cur.hang_q_dest_multileg && !cur.hang_q_dest_pathclear);
    }

    #[test]
    fn grid_is_four_by_four() {
        let tmp = std::env::temp_dir().join(format!("tk-hang-q-ab-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let seed_path = if PathBuf::from(DEFAULT_SEED_MODEL).is_file() {
            PathBuf::from(DEFAULT_SEED_MODEL)
        } else {
            let p = tmp.join("ab-seed.json");
            EvalCheckpoint::seed("ab-seed").save_path(&p).unwrap();
            p
        };
        let cfg = HangQAbGridConfig {
            seed_model: seed_path,
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        let (man, grid) = run_hang_q_ab_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 16);
        assert_eq!(man.entrants.len(), 16);
        for base in ["T150_P120_T12", "H120_P120_T15", "AVG_T150_H120", "C2K50A1"] {
            assert!(man.entrants.iter().any(|e| e.id == base));
            assert!(man.entrants.iter().any(|e| e.id == format!("{base}A")));
            assert!(man.entrants.iter().any(|e| e.id == format!("{base}B")));
            assert!(man.entrants.iter().any(|e| e.id == format!("{base}AB")));
        }

        let ctrl = EvalCheckpoint::load_path(tmp.join("T150_P120_T12.json")).unwrap();
        assert!(!ctrl.search_defaults.hang_q_dest_multileg);
        assert!(!ctrl.search_defaults.hang_q_dest_pathclear);
        let a = EvalCheckpoint::load_path(tmp.join("T150_P120_T12A.json")).unwrap();
        assert!(a.search_defaults.hang_q_dest_multileg);
        assert!(!a.search_defaults.hang_q_dest_pathclear);
        let b = EvalCheckpoint::load_path(tmp.join("T150_P120_T12B.json")).unwrap();
        assert!(!b.search_defaults.hang_q_dest_multileg);
        assert!(b.search_defaults.hang_q_dest_pathclear);
        let ab = EvalCheckpoint::load_path(tmp.join("T150_P120_T12AB.json")).unwrap();
        assert!(ab.search_defaults.hang_q_dest_multileg);
        assert!(ab.search_defaults.hang_q_dest_pathclear);
        assert!((a.weights.two_mover_mob_k - ctrl.weights.two_mover_mob_k).abs() < 1e-6);

        let c2 = EvalCheckpoint::load_path(tmp.join("C2K50A1.json")).unwrap();
        assert!((c2.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert_eq!(c2.weights.two_mover_mob_curve, 2);
        let seed = EvalWeights::seed();
        assert!((ctrl.weights.eg_tropism_scale - 1.2).abs() < 1e-6);
        assert!(
            (ctrl.weights.piece_value(PieceType::HookMover)
                - seed.piece_value(PieceType::HookMover))
            .abs()
                > 100.0,
            "T150 Hook should differ from current seed"
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
