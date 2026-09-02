//! AVG_P120_SEED_C2 and BASE_P120H50B75_C2 × ±S2 × {none, L, A, LA} plus leftover
//! history from q-rs and a pre-last-royal-check logic engine (22 agents).
//!
//! Experimental (16): each chassis with suffixes `""|"S2"` then `""|"L"|"A"|"LA"`.
//!
//! Baselines (q-rs leftovers as-is, no leftover `_C2` twins, plus pinned
//! `LOGIC_PRE_LRCHECK` at the parent of last-royal-in-check):
//! `BASE_T150C120`, `BASE_P120H75B60`, `LOGIC_HANGQ_ANY`, `BASE_T150C50`,
//! `BASE_H105O105`, `LOGIC_PRE_LRCHECK`.

use crate::eval::{EvalCheckpoint, EvalWeights, SearchDefaults};
use crate::piece::PieceType;
use crate::training::history::{
    extract_seed_at, load_seed_at, logic_binary_path, HistoryManifest, DEFAULT_MANIFEST,
};
use crate::training::q_rs_grid::build_chassis_weights;
use crate::training::top4_mix_grid::MATERIALS;
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/royal-s2-grid";
pub const DEFAULT_CHAMPS_OUT_DIR: &str = "models/royal-s2-champs-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

/// Agents with at least one knockout title in `royal-s2-swiss-20260828T064125Z`
/// (titles desc, then id). No logic-engine pins.
pub const TITLE_WINNERS: [&str; 11] = [
    "BASE_P120H75B60",
    "AVG_P120_SEED_C2S2",
    "AVG_P120_SEED_C2A",
    "AVG_P120_SEED_C2L",
    "AVG_P120_SEED_C2S2L",
    "AVG_P120_SEED_C2",
    "AVG_P120_SEED_C2LA",
    "BASE_P120H50B75_C2",
    "AVG_P120_SEED_C2S2A",
    "AVG_P120_SEED_C2S2LA",
    "BASE_T150C120",
];

pub const CHASSIS: [&str; 2] = ["AVG_P120_SEED_C2", "BASE_P120H50B75_C2"];

/// q-rs leftovers plus the pre-last-royal-check logic pin.
pub const BASELINES: [&str; 6] = [
    "BASE_T150C120",
    "BASE_P120H75B60",
    "LOGIC_HANGQ_ANY",
    "BASE_T150C50",
    "BASE_H105O105",
    "LOGIC_PRE_LRCHECK",
];

/// On-values when L / A suffixes are set (`k=0` remains off).
pub const LR_FLIGHT_K: f32 = 4000.0;
pub const TWO_MOVER_ALIGN_K: f32 = 80.0;
pub const TWO_MOVER_ALIGN_CAP: f32 = 400.0;

/// (`suffix`, s2, l, a)
pub const FACTORIAL: [(&str, bool, bool, bool); 8] = [
    ("", false, false, false),
    ("S2", true, false, false),
    ("L", false, true, false),
    ("A", false, false, true),
    ("LA", false, true, true),
    ("S2L", true, true, false),
    ("S2A", true, false, true),
    ("S2LA", true, true, true),
];

#[derive(Debug, Clone)]
pub struct RoyalS2GridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for RoyalS2GridConfig {
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
    pub q_own_large_only: bool,
    pub lr_flight_k: f32,
    pub two_mover_align_k: f32,
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

fn apply_la(weights: &EvalWeights, l: bool, a: bool) -> EvalWeights {
    let mut w = weights.clone();
    w.lr_flight_k = if l { LR_FLIGHT_K } else { 0.0 };
    if a {
        w.two_mover_align_k = TWO_MOVER_ALIGN_K;
        w.two_mover_align_cap = TWO_MOVER_ALIGN_CAP;
    } else {
        w.two_mover_align_k = 0.0;
        w.two_mover_align_cap = 0.0;
    }
    w
}

fn apply_s2(base: &SearchDefaults, s2: bool) -> SearchDefaults {
    let mut s = base.clone();
    s.q_own_large_only = s2;
    s
}

fn piece_map<'a>(
    maps: &'a HashMap<&str, HashMap<PieceType, f32>>,
    tag: &str,
) -> Result<&'a HashMap<PieceType, f32>, String> {
    maps.get(tag)
        .ok_or_else(|| format!("missing {tag} piece map"))
}

fn chassis_by_id<'a>(
    chassis: &'a [(String, EvalWeights)],
    id: &str,
) -> Result<&'a EvalWeights, String> {
    chassis
        .iter()
        .find(|(i, _)| i == id)
        .map(|(_, w)| w)
        .ok_or_else(|| format!("missing chassis {id}"))
}

fn push_cell(
    cells: &mut Vec<GridCell>,
    entrants: &mut Vec<TourneyEntrant>,
    out_dir: &std::path::Path,
    mut cp: EvalCheckpoint,
    id: String,
    base_id: &str,
    kind: &str,
) -> Result<(), String> {
    let model_path = out_dir.join(format!("{id}.json"));
    cp.name = id.clone();
    cp.save_path(&model_path)
        .map_err(|e| format!("save {}: {e}", model_path.display()))?;
    let model = model_path.display().to_string();
    cells.push(GridCell {
        id: id.clone(),
        model: model.clone(),
        base_id: base_id.into(),
        kind: kind.into(),
        q_own_large_only: cp.search_defaults.q_own_large_only,
        lr_flight_k: cp.weights.lr_flight_k,
        two_mover_align_k: cp.weights.two_mover_align_k,
    });
    entrants.push(TourneyEntrant {
        id,
        model,
        engine: None,
    });
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
    let dest = out_dir.join(format!("{id}.json"));
    extract_seed_at(hist.git_for(id)?, id, &dest)?;
    let model = dest.display().to_string();
    cells.push(GridCell {
        id: id.into(),
        model: model.clone(),
        base_id: id.into(),
        kind: "baseline".into(),
        q_own_large_only: false,
        lr_flight_k: 0.0,
        two_mover_align_k: 0.0,
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

/// Write 16 experimental twins + 6 leftover-history baselines.
pub fn run_royal_s2_grid(cfg: &RoyalS2GridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    let chassis_w = build_chassis_weights(base, t150, h120, h105);

    let mut cells = Vec::with_capacity(22);
    let mut entrants = Vec::with_capacity(22);

    let search0 = base_cp.search_defaults.clone();
    for &base_id in &CHASSIS {
        let weights = chassis_by_id(&chassis_w, base_id)?;
        for &(suffix, s2, l, a) in &FACTORIAL {
            let id = variant_id(base_id, suffix);
            let mut cp = base_cp.clone();
            cp.weights = apply_la(weights, l, a);
            cp.search_defaults = apply_s2(&search0, s2);
            push_cell(
                &mut cells,
                &mut entrants,
                &cfg.out_dir,
                cp,
                id,
                base_id,
                "experimental",
            )?;
        }
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

/// Same cells as [`run_royal_s2_grid`], then keep only [`TITLE_WINNERS`].
pub fn run_royal_s2_champs_grid(
    cfg: &RoyalS2GridConfig,
) -> Result<(TourneyManifest, GridFile), String> {
    let (mut man, mut grid) = run_royal_s2_grid(cfg)?;
    let keep: HashSet<&str> = TITLE_WINNERS.iter().copied().collect();
    man.entrants.retain(|e| keep.contains(e.id.as_str()));
    grid.cells.retain(|c| keep.contains(c.id.as_str()));
    if man.entrants.len() != TITLE_WINNERS.len() {
        return Err(format!(
            "champs filter: wrote {} entrants, want {}",
            man.entrants.len(),
            TITLE_WINNERS.len()
        ));
    }
    for e in &man.entrants {
        if e.engine.is_some() {
            return Err(format!("{} should not pin a logic engine", e.id));
        }
    }

    let grid_path = cfg.out_dir.join("grid.json");
    fs::write(
        &grid_path,
        serde_json::to_string_pretty(&grid).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {}: {e}", grid_path.display()))?;
    let man_path = cfg.out_dir.join("manifest.json");
    fs::write(
        &man_path,
        serde_json::to_string_pretty(&man).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {}: {e}", man_path.display()))?;
    Ok((man, grid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variant_ids() {
        assert_eq!(variant_id("AVG_P120_SEED_C2", ""), "AVG_P120_SEED_C2");
        assert_eq!(
            variant_id("AVG_P120_SEED_C2", "S2LA"),
            "AVG_P120_SEED_C2S2LA"
        );
        assert_eq!(
            variant_id("BASE_P120H50B75_C2", "S2L"),
            "BASE_P120H50B75_C2S2L"
        );
    }

    #[test]
    fn grid_writes_22_unique_entrants() {
        let tmp = std::env::temp_dir().join(format!("tk-royal-s2-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let seed_path = if PathBuf::from(DEFAULT_SEED_MODEL).is_file() {
            PathBuf::from(DEFAULT_SEED_MODEL)
        } else {
            let p = tmp.join("ab-seed.json");
            EvalCheckpoint::seed("ab-seed").save_path(&p).unwrap();
            p
        };
        let cfg = RoyalS2GridConfig {
            seed_model: seed_path,
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        let (man, grid) = run_royal_s2_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 22);
        assert_eq!(man.entrants.len(), 22);
        let ids: Vec<_> = man.entrants.iter().map(|e| e.id.as_str()).collect();
        let uniq: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(uniq.len(), ids.len(), "duplicate ids: {ids:?}");

        for &base_id in &CHASSIS {
            for &(suffix, ..) in &FACTORIAL {
                let id = variant_id(base_id, suffix);
                assert!(ids.contains(&id.as_str()), "missing {id}");
            }
        }
        for id in BASELINES {
            assert!(ids.contains(&id), "missing leftover {id}");
        }
        for id in [
            "SEED",
            "H120_P120_T15",
            "H120_P120_T15_C2",
            "C2K50A1",
            "BASE_T150C120_C2",
            "BASE_PRELOUD",
            "LOGIC_H105",
        ] {
            assert!(!ids.contains(&id), "unexpected {id}");
        }

        let plain = EvalCheckpoint::load_path(tmp.join("AVG_P120_SEED_C2.json")).unwrap();
        assert!(!plain.search_defaults.q_own_large_only);
        assert!((plain.weights.lr_flight_k - 0.0).abs() < 1e-6);
        assert!((plain.weights.two_mover_align_k - 0.0).abs() < 1e-6);
        assert!((plain.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert_eq!(plain.weights.two_mover_mob_curve, 2);

        let s2la = EvalCheckpoint::load_path(tmp.join("AVG_P120_SEED_C2S2LA.json")).unwrap();
        assert!(s2la.search_defaults.q_own_large_only);
        assert!((s2la.weights.lr_flight_k - LR_FLIGHT_K).abs() < 1e-6);
        assert!((s2la.weights.two_mover_align_k - TWO_MOVER_ALIGN_K).abs() < 1e-6);

        let p120 = EvalCheckpoint::load_path(tmp.join("BASE_P120H50B75_C2S2LA.json")).unwrap();
        assert!(p120.search_defaults.q_own_large_only);
        assert!((p120.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert!((p120.weights.lr_flight_k - LR_FLIGHT_K).abs() < 1e-6);

        let hist = EvalCheckpoint::load_path(tmp.join("BASE_T150C120.json")).unwrap();
        assert!((hist.weights.two_mover_mob_k - 0.0).abs() < 1e-6);

        let hangq = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_HANGQ_ANY")
            .expect("LOGIC_HANGQ_ANY");
        assert!(hangq.engine.as_ref().unwrap().contains("LOGIC_HANGQ_ANY"));
        let pre = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_PRE_LRCHECK")
            .expect("LOGIC_PRE_LRCHECK");
        assert!(pre.engine.as_ref().unwrap().contains("LOGIC_PRE_LRCHECK"));

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn champs_grid_writes_title_winners_only() {
        let tmp = std::env::temp_dir().join(format!("tk-royal-s2-champs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let seed_path = if PathBuf::from(DEFAULT_SEED_MODEL).is_file() {
            PathBuf::from(DEFAULT_SEED_MODEL)
        } else {
            let p = tmp.join("ab-seed.json");
            EvalCheckpoint::seed("ab-seed").save_path(&p).unwrap();
            p
        };
        let cfg = RoyalS2GridConfig {
            seed_model: seed_path,
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        let (man, grid) = run_royal_s2_champs_grid(&cfg).expect("champs");
        assert_eq!(man.entrants.len(), 11);
        assert_eq!(grid.cells.len(), 11);
        let ids: HashSet<_> = man.entrants.iter().map(|e| e.id.as_str()).collect();
        for id in TITLE_WINNERS {
            assert!(ids.contains(id), "missing {id}");
        }
        assert!(!ids.contains("LOGIC_HANGQ_ANY"));
        assert!(!ids.contains("LOGIC_PRE_LRCHECK"));
        assert!(!ids.contains("BASE_P120H50B75_C2L"));
        assert!(man.entrants.iter().all(|e| e.engine.is_none()));
        let _ = fs::remove_dir_all(&tmp);
    }
}
