//! Independent R / S1 / S2 search twins on seven experimental chassis,
//! plus five leftover-history baselines (33 agents).
//!
//! Chassis (current + R + S1 + S2):
//! `H120_P120_T15`, `AVG_P120_SEED_C2`, `SEED`, `BASE_P120H50B75_C2`,
//! `T150_P120_T12_C2`, `C2K100A1D50`, `C2K50A1`.
//!
//! Baselines keep current search (hang-q A+B on, R/S off):
//! `BASE_T150C120`, `BASE_P120H75B60`, `LOGIC_HANGQ_ANY`, `BASE_T150C50`,
//! `BASE_H105O105`. No leftover `_C2` twins.

use crate::eval::{EvalCheckpoint, EvalWeights, SearchDefaults};
use crate::piece::PieceType;
use crate::training::history::{
    extract_seed_at, load_seed_at, logic_binary_path, HistoryManifest, DEFAULT_MANIFEST,
};
use crate::training::top4_mix_grid::{
    apply_cell, apply_two_mob_extra, mix_weights, MATERIALS, PSTS,
};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/q-rs-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// Experimental weight chassis, rating order from top11-c2 (C2K50A1 not D50).
pub const CHASSIS: [&str; 7] = [
    "H120_P120_T15",
    "AVG_P120_SEED_C2",
    "SEED",
    "BASE_P120H50B75_C2",
    "T150_P120_T12_C2",
    "C2K100A1D50",
    "C2K50A1",
];

/// Leftover history parents, no leftover C2s, no PRELOUD / LOGIC_H105.
pub const BASELINES: [&str; 5] = [
    "BASE_T150C120",
    "BASE_P120H75B60",
    "LOGIC_HANGQ_ANY",
    "BASE_T150C50",
    "BASE_H105O105",
];

/// (`suffix`, R, S1-open, S1-recapture, S2)
pub const SEARCH_VARIANTS: [(&str, bool, bool, bool, bool); 4] = [
    ("", false, false, false, false),
    ("R", true, false, false, false),
    ("S1", false, true, true, false),
    ("S2", false, false, false, true),
];

#[derive(Debug, Clone)]
pub struct QRsGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for QRsGridConfig {
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
    pub kind: String,
    pub q_open_large_mover: bool,
    pub q_open_any_capture: bool,
    pub q_recapture_only: bool,
    pub q_own_large_only: bool,
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

pub fn apply_rs_search(
    base: &SearchDefaults,
    open_large: bool,
    open_any: bool,
    recapture_only: bool,
    own_large: bool,
) -> SearchDefaults {
    let mut s = base.clone();
    s.q_open_large_mover = open_large;
    s.q_open_any_capture = open_any;
    s.q_recapture_only = recapture_only;
    s.q_own_large_only = own_large;
    s
}

fn piece_map<'a>(
    maps: &'a HashMap<&str, HashMap<PieceType, f32>>,
    tag: &str,
) -> Result<&'a HashMap<PieceType, f32>, String> {
    maps.get(tag)
        .ok_or_else(|| format!("missing {tag} piece map"))
}

/// Rebuild the seven experimental chassis from material maps + seed extras.
pub fn build_chassis_weights(
    base: &EvalWeights,
    t150: &HashMap<PieceType, f32>,
    h120: &HashMap<PieceType, f32>,
    h105: &HashMap<PieceType, f32>,
) -> Vec<(String, EvalWeights)> {
    let p120 = &PSTS[1];
    let b65 = &PSTS[2];
    let base_p120 = apply_cell(base, h105, p120, 1.5);
    let seed = apply_cell(base, h105, b65, 1.2);
    vec![
        ("H120_P120_T15".into(), apply_cell(base, h120, p120, 1.5)),
        (
            "AVG_P120_SEED_C2".into(),
            apply_two_mob_extra(&mix_weights(&base_p120, &seed), 50.0, 0.0),
        ),
        ("SEED".into(), seed.clone()),
        (
            "BASE_P120H50B75_C2".into(),
            apply_two_mob_extra(&base_p120, 50.0, 0.0),
        ),
        (
            "T150_P120_T12_C2".into(),
            apply_two_mob_extra(&apply_cell(base, t150, p120, 1.2), 50.0, 0.0),
        ),
        ("C2K100A1D50".into(), apply_two_mob_extra(base, 100.0, 50.0)),
        ("C2K50A1".into(), apply_two_mob_extra(base, 50.0, 0.0)),
    ]
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
    for &(suffix, r, s1_open, s1_rec, s2) in &SEARCH_VARIANTS {
        let id = variant_id(base_id, suffix);
        let model_path = out_dir.join(format!("{id}.json"));
        cp.name = id.clone();
        cp.weights = weights.clone();
        cp.search_defaults = apply_rs_search(&search0, r, s1_open, s1_rec, s2);
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        let model = model_path.display().to_string();
        cells.push(GridCell {
            id: id.clone(),
            model: model.clone(),
            base_id: base_id.into(),
            kind: "chassis".into(),
            q_open_large_mover: r,
            q_open_any_capture: s1_open,
            q_recapture_only: s1_rec,
            q_own_large_only: s2,
        });
        entrants.push(TourneyEntrant {
            id,
            model,
            engine: None,
        });
    }
    Ok(())
}

fn push_baseline(
    cells: &mut Vec<GridCell>,
    entrants: &mut Vec<TourneyEntrant>,
    hist: &HistoryManifest,
    out_dir: &std::path::Path,
    id: &str,
    engine: bool,
) -> Result<(), String> {
    let rev = hist.git_for(id)?;
    let dest = out_dir.join(format!("{id}.json"));
    extract_seed_at(rev, id, &dest)?;
    let model = dest.display().to_string();
    cells.push(GridCell {
        id: id.into(),
        model: model.clone(),
        base_id: id.into(),
        kind: "baseline".into(),
        q_open_large_mover: false,
        q_open_any_capture: false,
        q_recapture_only: false,
        q_own_large_only: false,
    });
    entrants.push(TourneyEntrant {
        id: id.into(),
        model,
        engine: if engine {
            Some(logic_binary_path(id).display().to_string())
        } else {
            None
        },
    });
    Ok(())
}

/// Write 28 chassis twins + 5 history baselines and a knockout manifest.
pub fn run_q_rs_grid(cfg: &QRsGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    let chassis = build_chassis_weights(base, t150, h120, h105);
    debug_assert_eq!(
        chassis
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        CHASSIS
    );

    let mut cells = Vec::with_capacity(33);
    let mut entrants = Vec::with_capacity(33);

    for (id, weights) in chassis {
        let mut cp = base_cp.clone();
        cp.weights = weights;
        push_variants(&mut cells, &mut entrants, &id, &cfg.out_dir, cp)?;
    }

    for &id in &BASELINES {
        push_baseline(
            &mut cells,
            &mut entrants,
            &hist,
            &cfg.out_dir,
            id,
            id.starts_with("LOGIC_"),
        )?;
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

    #[test]
    fn variant_ids() {
        assert_eq!(variant_id("C2K50A1", ""), "C2K50A1");
        assert_eq!(variant_id("C2K50A1", "R"), "C2K50A1R");
        assert_eq!(variant_id("H120_P120_T15", "S1"), "H120_P120_T15S1");
        assert_eq!(variant_id("SEED", "S2"), "SEEDS2");
    }

    #[test]
    fn search_defaults_set_only_requested_flags() {
        let r = apply_rs_search(&SearchDefaults::default(), true, false, false, false);
        assert!(r.q_open_large_mover);
        assert!(!r.q_open_any_capture && !r.q_recapture_only && !r.q_own_large_only);
        assert!(r.hang_q_dest_multileg && r.hang_q_dest_pathclear);
        let s1 = apply_rs_search(&SearchDefaults::default(), false, true, true, false);
        assert!(s1.q_open_any_capture && s1.q_recapture_only);
        assert!(!s1.q_open_large_mover && !s1.q_own_large_only);
        let s2 = apply_rs_search(&SearchDefaults::default(), false, false, false, true);
        assert!(s2.q_own_large_only);
        assert!(!s2.q_open_large_mover && !s2.q_open_any_capture);
        let cur = apply_rs_search(&SearchDefaults::default(), false, false, false, false);
        assert!(
            !cur.q_open_large_mover
                && !cur.q_open_any_capture
                && !cur.q_recapture_only
                && !cur.q_own_large_only
        );
    }

    #[test]
    fn grid_writes_33_unique_entrants() {
        let tmp = std::env::temp_dir().join(format!("tk-q-rs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let seed_path = if PathBuf::from(DEFAULT_SEED_MODEL).is_file() {
            PathBuf::from(DEFAULT_SEED_MODEL)
        } else {
            let p = tmp.join("ab-seed.json");
            EvalCheckpoint::seed("ab-seed").save_path(&p).unwrap();
            p
        };
        let cfg = QRsGridConfig {
            seed_model: seed_path,
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        let (man, grid) = run_q_rs_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 33);
        assert_eq!(man.entrants.len(), 33);
        let ids: Vec<_> = man.entrants.iter().map(|e| e.id.as_str()).collect();
        let uniq: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(uniq.len(), ids.len(), "duplicate ids: {ids:?}");

        for base in CHASSIS {
            assert!(ids.contains(&base), "missing {base}");
            let r = format!("{base}R");
            let s1 = format!("{base}S1");
            let s2 = format!("{base}S2");
            assert!(ids.contains(&r.as_str()), "missing {r}");
            assert!(ids.contains(&s1.as_str()), "missing {s1}");
            assert!(ids.contains(&s2.as_str()), "missing {s2}");
        }
        for id in BASELINES {
            assert!(ids.contains(&id), "missing baseline {id}");
        }
        for id in [
            "BASE_PRELOUD",
            "BASE_PRELOUD_C2",
            "BASE_T150C120_C2",
            "LOGIC_H105",
            "C2K50A1D50",
        ] {
            assert!(!ids.contains(&id), "unexpected {id}");
        }

        let ctrl = EvalCheckpoint::load_path(tmp.join("SEED.json")).unwrap();
        assert!(!ctrl.search_defaults.q_open_large_mover);
        assert!(!ctrl.search_defaults.q_open_any_capture);
        assert!(!ctrl.search_defaults.q_recapture_only);
        assert!(!ctrl.search_defaults.q_own_large_only);
        assert!(ctrl.search_defaults.hang_q_dest_multileg);
        assert!(ctrl.search_defaults.hang_q_dest_pathclear);

        let r = EvalCheckpoint::load_path(tmp.join("SEEDR.json")).unwrap();
        assert!(r.search_defaults.q_open_large_mover);
        assert!(!r.search_defaults.q_open_any_capture);
        assert!(!r.search_defaults.q_recapture_only);
        assert!(!r.search_defaults.q_own_large_only);

        let s1 = EvalCheckpoint::load_path(tmp.join("SEEDS1.json")).unwrap();
        assert!(s1.search_defaults.q_open_any_capture);
        assert!(s1.search_defaults.q_recapture_only);
        assert!(!s1.search_defaults.q_open_large_mover);
        assert!(!s1.search_defaults.q_own_large_only);

        let s2 = EvalCheckpoint::load_path(tmp.join("SEEDS2.json")).unwrap();
        assert!(s2.search_defaults.q_own_large_only);
        assert!(!s2.search_defaults.q_open_large_mover);
        assert!(!s2.search_defaults.q_open_any_capture);

        let c2 = EvalCheckpoint::load_path(tmp.join("C2K50A1.json")).unwrap();
        assert!((c2.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert_eq!(c2.weights.two_mover_mob_curve, 2);

        let logic = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_HANGQ_ANY")
            .expect("LOGIC_HANGQ_ANY");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_HANGQ_ANY"));

        let _ = fs::remove_dir_all(&tmp);
    }
}
