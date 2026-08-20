//! 3×3×2 mix of the knockout top-4 axes, plus leftover history, pairwise averages,
//! and seed two-mover mobility extras (C2K100A1 / C2K50A1 and material-discount twins).
//!
//! Axes (from BASE_T150C50 / BASE_H120O80 / BASE_P120H50B75 / SEED):
//! - Material: T150, H120, H105 (P120/SEED share H105 piece values)
//! - Fast PST: OLD (B50/H50), P120 (B75/H50), B65 (B65/H75); promo stays 120%
//! - Tropism: T15 (1.5) / T12 (1.2)
//!
//! Five cells reuse history/SEED ids. `BASE_H105O105` is H105×OLD×T15 (not a top-4
//! agent, but the same weights as that corner). LOGIC_B65T12 and LOGIC_TROPISM are
//! omitted; remaining history is PRELOUD, T150C120, P120H75B60, LOGIC_H105.

use crate::eval::{
    is_range_two_mover, seed_rank_factors_fast_params, EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES,
};
use crate::piece::PieceType;
use crate::training::history::{
    append_history_entrants_except, load_seed_at, HistoryManifest, DEFAULT_MANIFEST,
};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use crate::training::two_mob_grid::apply_two_mob_cell;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

pub const DEFAULT_OUT_DIR: &str = "models/top4-mix-grid";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

pub const MATERIALS: [MatAxis; 3] = [
    MatAxis {
        tag: "T150",
        history_id: "BASE_T150C50",
    },
    MatAxis {
        tag: "H120",
        history_id: "BASE_H120O80",
    },
    MatAxis {
        tag: "H105",
        history_id: "BASE_H105O105",
    },
];

pub const PSTS: [PstAxis; 3] = [
    PstAxis {
        tag: "OLD",
        back: 0.50,
        opp_half_frac: 0.50,
        promo: 1.2,
    },
    PstAxis {
        tag: "P120",
        back: 0.75,
        opp_half_frac: 0.50,
        promo: 1.2,
    },
    PstAxis {
        tag: "B65",
        back: 0.65,
        opp_half_frac: 0.75,
        promo: 1.2,
    },
];

pub const TROPISMS: [f32; 2] = [1.5, 1.2];

/// Knockout top 4; pairwise averages use this order.
pub const TOP4: [&str; 4] = ["BASE_T150C50", "BASE_H120O80", "BASE_P120H50B75", "SEED"];

/// History ids already represented as grid cells, plus the two dropped LOGIC engines.
pub const SKIP_HISTORY: &[&str] = &[
    "BASE_T150C50",
    "BASE_H120O80",
    "BASE_H105O105",
    "BASE_P120H50B75",
    "LOGIC_TROPISM",
    "LOGIC_B65T12",
];

/// Seed-chassis C2×A1 mobility extras: (k, two-mover material discount).
pub const TWO_MOB_EXTRAS: [(f32, f32); 4] =
    [(100.0, 0.0), (50.0, 0.0), (100.0, 50.0), (50.0, 25.0)];

#[derive(Debug, Clone, Copy)]
pub struct MatAxis {
    pub tag: &'static str,
    pub history_id: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct PstAxis {
    pub tag: &'static str,
    pub back: f32,
    pub opp_half_frac: f32,
    pub promo: f32,
}

#[derive(Debug, Clone)]
pub struct Top4MixGridConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub history_manifest: PathBuf,
}

impl Default for Top4MixGridConfig {
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
    pub material: Option<String>,
    pub pst: Option<String>,
    pub eg_tropism_scale: Option<f32>,
    pub mix_a: Option<String>,
    pub mix_b: Option<String>,
    pub two_mover_mob_k: Option<f32>,
    pub two_mover_discount: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GridFile {
    pub seed_model: String,
    pub cells: Vec<GridCell>,
}

pub fn trop_tag(scale: f32) -> String {
    format!("T{}", (scale * 10.0).round() as u32)
}

pub fn systematic_id(mat: &str, pst: &str, trop: f32) -> String {
    format!("{mat}_{pst}_{}", trop_tag(trop))
}

pub fn cell_id(mat: &str, pst: &str, trop: f32) -> String {
    match systematic_id(mat, pst, trop).as_str() {
        "T150_OLD_T15" => "BASE_T150C50".into(),
        "H120_OLD_T15" => "BASE_H120O80".into(),
        "H105_OLD_T15" => "BASE_H105O105".into(),
        "H105_P120_T15" => "BASE_P120H50B75".into(),
        "H105_B65_T12" => "SEED".into(),
        other => other.to_string(),
    }
}

pub fn short_top4(id: &str) -> &str {
    match id {
        "BASE_T150C50" => "T150",
        "BASE_H120O80" => "H120",
        "BASE_P120H50B75" => "P120",
        "SEED" => "SEED",
        other => other,
    }
}

pub fn avg_id(a: &str, b: &str) -> String {
    format!("AVG_{}_{}", short_top4(a), short_top4(b))
}

pub fn two_mob_extra_id(k: f32, discount: f32) -> String {
    let base = format!("C2K{}A1", k.round() as u32);
    if discount == 0.0 {
        base
    } else {
        format!("{base}D{}", discount.round() as u32)
    }
}

/// C2 A1 mobility on `base`, then subtract `discount` from every range two-mover.
pub fn apply_two_mob_extra(base: &EvalWeights, k: f32, discount: f32) -> EvalWeights {
    let mut w = apply_two_mob_cell(base, 2, k, 1);
    if discount != 0.0 {
        for &pt in ALL_PIECE_TYPES {
            if is_range_two_mover(pt) {
                if let Some(v) = w.piece.get_mut(&pt) {
                    *v -= discount;
                }
            }
        }
        w.rebuild_piece_value_table();
    }
    w
}

pub fn apply_cell(
    base: &EvalWeights,
    piece: &HashMap<PieceType, f32>,
    pst: &PstAxis,
    trop: f32,
) -> EvalWeights {
    let mut w = base.clone();
    w.piece = piece.clone();
    let fast = seed_rank_factors_fast_params(pst.back, pst.opp_half_frac, pst.promo).to_vec();
    w.rank_factor_fast = fast.clone();
    w.rank_factor = fast;
    w.eg_tropism_scale = trop;
    w.rebuild_piece_value_table();
    w
}

pub fn mix_weights(a: &EvalWeights, b: &EvalWeights) -> EvalWeights {
    let mut w = a.clone();
    w.piece = mix_piece_map(&a.piece, &b.piece);
    w.royal_alive = mix_i32(a.royal_alive, b.royal_alive);
    w.sole_royal_factor = mix_i32(a.sole_royal_factor, b.sole_royal_factor);
    w.royal_bonus_by_count = mix_vec_i32(&a.royal_bonus_by_count, &b.royal_bonus_by_count);
    w.de_advance = mix_i32(a.de_advance, b.de_advance);
    w.undeveloped_home = mix_i32(a.undeveloped_home, b.undeveloped_home);
    w.advance = mix_i32(a.advance, b.advance);
    w.rank_factor = mix_vec_f32(&a.rank_factor, &b.rank_factor);
    w.rank_factor_fast = mix_vec_f32(&a.rank_factor_fast, &b.rank_factor_fast);
    w.rank_factor_slow = mix_vec_f32(&a.rank_factor_slow, &b.rank_factor_slow);
    w.file_factor = mix_vec_f32(&a.file_factor, &b.file_factor);
    w.eg_tropism_scale = mix_f32(a.eg_tropism_scale, b.eg_tropism_scale);
    w.eg_tropism_cap = mix_f32(a.eg_tropism_cap, b.eg_tropism_cap);
    w.eg_density_n = mix_f32(a.eg_density_n, b.eg_density_n);
    w.eg_tropism_d_ref = mix_f32(a.eg_tropism_d_ref, b.eg_tropism_d_ref);
    w.eg_ahead_min = mix_f32(a.eg_ahead_min, b.eg_ahead_min);
    w.eg_tropism_short_scale = mix_f32(a.eg_tropism_short_scale, b.eg_tropism_short_scale);
    w.eg_tropism_range_scale = mix_f32(a.eg_tropism_range_scale, b.eg_tropism_range_scale);
    w.eg_tropism_topk = mix_u32(a.eg_tropism_topk, b.eg_tropism_topk);
    w.eg_tropism_tail_scale = mix_f32(a.eg_tropism_tail_scale, b.eg_tropism_tail_scale);
    w.two_mover_mob_k = mix_f32(a.two_mover_mob_k, b.two_mover_mob_k);
    w.two_mover_mob_curve = mix_u8(a.two_mover_mob_curve, b.two_mover_mob_curve);
    w.two_mover_mob_apply = mix_u8(a.two_mover_mob_apply, b.two_mover_mob_apply);
    w.noise_scale = (a.noise_scale + b.noise_scale) / 2.0;
    w.mate_score = mix_i32(a.mate_score, b.mate_score);
    w.rebuild_piece_value_table();
    w
}

fn mix_f32(a: f32, b: f32) -> f32 {
    (a + b) / 2.0
}

fn mix_i32(a: i32, b: i32) -> i32 {
    ((a as f32 + b as f32) / 2.0).round() as i32
}

fn mix_u32(a: u32, b: u32) -> u32 {
    ((a as f32 + b as f32) / 2.0).round() as u32
}

fn mix_u8(a: u8, b: u8) -> u8 {
    ((a as f32 + b as f32) / 2.0).round() as u8
}

fn mix_vec_f32(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| mix_f32(*a.get(i).unwrap_or(&1.0), *b.get(i).unwrap_or(&1.0)))
        .collect()
}

fn mix_vec_i32(a: &[i32], b: &[i32]) -> Vec<i32> {
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| mix_i32(*a.get(i).unwrap_or(&0), *b.get(i).unwrap_or(&0)))
        .collect()
}

fn mix_piece_map(
    a: &HashMap<PieceType, f32>,
    b: &HashMap<PieceType, f32>,
) -> HashMap<PieceType, f32> {
    let mut keys: HashSet<PieceType> = a.keys().copied().collect();
    keys.extend(b.keys().copied());
    let mut out = HashMap::with_capacity(keys.len());
    for k in keys {
        match (a.get(&k), b.get(&k)) {
            (Some(&va), Some(&vb)) => {
                out.insert(k, mix_f32(va, vb));
            }
            (Some(&va), None) => {
                out.insert(k, va);
            }
            (None, Some(&vb)) => {
                out.insert(k, vb);
            }
            (None, None) => {}
        }
    }
    out
}

fn push_entrant(entrants: &mut Vec<TourneyEntrant>, id: String, model: String) {
    entrants.push(TourneyEntrant {
        id,
        model,
        engine: None,
    });
}

/// Write 18 mix cells + 6 pairwise averages + 4 two-mob extras + leftover history.
pub fn run_top4_mix_grid(cfg: &Top4MixGridConfig) -> Result<(TourneyManifest, GridFile), String> {
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

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let mut cells = Vec::with_capacity(28);
    let mut entrants = Vec::with_capacity(32);
    let mut by_id: HashMap<String, EvalWeights> = HashMap::new();

    for mat in &MATERIALS {
        let piece = piece_maps
            .get(mat.tag)
            .ok_or_else(|| format!("missing piece map {}", mat.tag))?;
        for pst in &PSTS {
            for &trop in &TROPISMS {
                let id = cell_id(mat.tag, pst.tag, trop);
                let model_path = cfg.out_dir.join(format!("{id}.json"));
                let mut cp = base_cp.clone();
                cp.name = id.clone();
                cp.weights = apply_cell(base, piece, pst, trop);
                cp.save_path(&model_path)
                    .map_err(|e| format!("save {}: {e}", model_path.display()))?;
                by_id.insert(id.clone(), cp.weights.clone());
                let model = model_path.display().to_string();
                cells.push(GridCell {
                    id: id.clone(),
                    model: model.clone(),
                    kind: "mix".into(),
                    material: Some(mat.tag.into()),
                    pst: Some(pst.tag.into()),
                    eg_tropism_scale: Some(trop),
                    mix_a: None,
                    mix_b: None,
                    two_mover_mob_k: None,
                    two_mover_discount: None,
                });
                push_entrant(&mut entrants, id, model);
            }
        }
    }

    for i in 0..TOP4.len() {
        for j in (i + 1)..TOP4.len() {
            let a_id = TOP4[i];
            let b_id = TOP4[j];
            let wa = by_id
                .get(a_id)
                .ok_or_else(|| format!("missing mix cell {a_id}"))?;
            let wb = by_id
                .get(b_id)
                .ok_or_else(|| format!("missing mix cell {b_id}"))?;
            let id = avg_id(a_id, b_id);
            let model_path = cfg.out_dir.join(format!("{id}.json"));
            let mut cp = base_cp.clone();
            cp.name = id.clone();
            cp.weights = mix_weights(wa, wb);
            cp.save_path(&model_path)
                .map_err(|e| format!("save {}: {e}", model_path.display()))?;
            let model = model_path.display().to_string();
            cells.push(GridCell {
                id: id.clone(),
                model: model.clone(),
                kind: "avg".into(),
                material: None,
                pst: None,
                eg_tropism_scale: Some(cp.weights.eg_tropism_scale),
                mix_a: Some(a_id.into()),
                mix_b: Some(b_id.into()),
                two_mover_mob_k: None,
                two_mover_discount: None,
            });
            push_entrant(&mut entrants, id, model);
        }
    }

    for &(k, discount) in &TWO_MOB_EXTRAS {
        let id = two_mob_extra_id(k, discount);
        let model_path = cfg.out_dir.join(format!("{id}.json"));
        let mut cp = base_cp.clone();
        cp.name = id.clone();
        cp.weights = apply_two_mob_extra(base, k, discount);
        cp.save_path(&model_path)
            .map_err(|e| format!("save {}: {e}", model_path.display()))?;
        let model = model_path.display().to_string();
        cells.push(GridCell {
            id: id.clone(),
            model: model.clone(),
            kind: "two_mob".into(),
            material: None,
            pst: None,
            eg_tropism_scale: None,
            mix_a: None,
            mix_b: None,
            two_mover_mob_k: Some(k),
            two_mover_discount: Some(discount),
        });
        push_entrant(&mut entrants, id, model);
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
    use crate::eval::EvalWeights;

    #[test]
    fn eighteen_cells_and_aliases() {
        let mut ids = Vec::new();
        for mat in &MATERIALS {
            for pst in &PSTS {
                for &trop in &TROPISMS {
                    ids.push(cell_id(mat.tag, pst.tag, trop));
                }
            }
        }
        assert_eq!(ids.len(), 18);
        let set: HashSet<_> = ids.iter().cloned().collect();
        assert_eq!(set.len(), 18);
        assert!(set.contains("BASE_T150C50"));
        assert!(set.contains("BASE_H120O80"));
        assert!(set.contains("BASE_P120H50B75"));
        assert!(set.contains("SEED"));
        assert!(set.contains("BASE_H105O105"));
        assert!(set.contains("T150_P120_T12"));
        assert!(set.contains("H120_B65_T15"));
        assert_eq!(ids.iter().filter(|id| *id == "SEED").count(), 1);
    }

    #[test]
    fn pairwise_ids_are_six_and_ordered() {
        let mut ids = Vec::new();
        for i in 0..TOP4.len() {
            for j in (i + 1)..TOP4.len() {
                ids.push(avg_id(TOP4[i], TOP4[j]));
            }
        }
        assert_eq!(
            ids,
            vec![
                "AVG_T150_H120",
                "AVG_T150_P120",
                "AVG_T150_SEED",
                "AVG_H120_P120",
                "AVG_H120_SEED",
                "AVG_P120_SEED",
            ]
        );
    }

    #[test]
    fn apply_and_mix_are_distinct_from_parents() {
        let seed = EvalWeights::seed();
        let t150_piece = {
            let mut p = seed.piece.clone();
            p.insert(PieceType::HookMover, 3300.0);
            p
        };
        let h105_piece = seed.piece.clone();
        let a = apply_cell(&seed, &t150_piece, &PSTS[0], 1.5);
        let b = apply_cell(&seed, &h105_piece, &PSTS[2], 1.2);
        let m = mix_weights(&a, &b);
        assert!((a.piece[&PieceType::HookMover] - 3300.0).abs() < 1e-3);
        assert!((a.rank_factor_fast[0] - 0.50).abs() < 1e-5);
        assert!((a.eg_tropism_scale - 1.5).abs() < 1e-6);
        assert!((b.rank_factor_fast[0] - 0.65).abs() < 1e-5);
        assert!((b.eg_tropism_scale - 1.2).abs() < 1e-6);
        let hook_mid = (a.piece[&PieceType::HookMover] + b.piece[&PieceType::HookMover]) / 2.0;
        assert!((m.piece[&PieceType::HookMover] - hook_mid).abs() < 1e-3);
        assert!((m.eg_tropism_scale - 1.35).abs() < 1e-5);
        assert!((m.rank_factor_fast[0] - 0.575).abs() < 1e-5);
    }

    #[test]
    fn two_mob_extra_ids_and_discount() {
        assert_eq!(two_mob_extra_id(100.0, 0.0), "C2K100A1");
        assert_eq!(two_mob_extra_id(50.0, 0.0), "C2K50A1");
        assert_eq!(two_mob_extra_id(100.0, 50.0), "C2K100A1D50");
        assert_eq!(two_mob_extra_id(50.0, 25.0), "C2K50A1D25");
        let seed = EvalWeights::seed();
        let plain = apply_two_mob_extra(&seed, 100.0, 0.0);
        let d50 = apply_two_mob_extra(&seed, 100.0, 50.0);
        let d25 = apply_two_mob_extra(&seed, 50.0, 25.0);
        assert!((plain.two_mover_mob_k - 100.0).abs() < 1e-6);
        assert_eq!(plain.two_mover_mob_curve, 2);
        assert_eq!(plain.two_mover_mob_apply, 1);
        assert_eq!(
            plain.piece[&PieceType::HookMover],
            seed.piece[&PieceType::HookMover]
        );
        assert!(
            (d50.piece[&PieceType::HookMover] - (seed.piece[&PieceType::HookMover] - 50.0)).abs()
                < 1e-3
        );
        assert!(
            (d50.piece[&PieceType::Tengu] - (seed.piece[&PieceType::Tengu] - 50.0)).abs() < 1e-3
        );
        assert_eq!(
            d50.piece[&PieceType::GreatGeneral],
            seed.piece[&PieceType::GreatGeneral]
        );
        assert!(
            (d25.piece[&PieceType::HookMover] - (seed.piece[&PieceType::HookMover] - 25.0)).abs()
                < 1e-3
        );
        assert!((d25.two_mover_mob_k - 50.0).abs() < 1e-6);
    }

    #[test]
    fn grid_writes_32_unique_entrants() {
        let tmp = std::env::temp_dir().join(format!("tk-top4-mix-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let cfg = Top4MixGridConfig {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: tmp.clone(),
            history_manifest: PathBuf::from(DEFAULT_MANIFEST),
        };
        if !cfg.seed_model.is_file() {
            EvalCheckpoint::seed("ab-seed")
                .save_path(&cfg.seed_model)
                .unwrap();
        }
        let (man, grid) = run_top4_mix_grid(&cfg).expect("grid");
        assert_eq!(grid.cells.len(), 28);
        assert_eq!(grid.cells.iter().filter(|c| c.kind == "mix").count(), 18);
        assert_eq!(grid.cells.iter().filter(|c| c.kind == "avg").count(), 6);
        assert_eq!(grid.cells.iter().filter(|c| c.kind == "two_mob").count(), 4);
        let ids: Vec<_> = man.entrants.iter().map(|e| e.id.as_str()).collect();
        let uniq: HashSet<_> = ids.iter().copied().collect();
        assert_eq!(uniq.len(), ids.len(), "duplicate ids: {ids:?}");
        assert_eq!(man.entrants.len(), 32);
        for id in TOP4 {
            assert!(ids.contains(&id), "missing {id}");
        }
        assert!(ids.contains(&"BASE_H105O105"));
        assert!(ids.contains(&"AVG_P120_SEED"));
        assert!(ids.contains(&"BASE_PRELOUD"));
        assert!(ids.contains(&"BASE_T150C120"));
        assert!(ids.contains(&"BASE_P120H75B60"));
        assert!(ids.contains(&"LOGIC_H105"));
        assert!(!ids.contains(&"LOGIC_B65T12"));
        assert!(!ids.contains(&"LOGIC_TROPISM"));
        let logic = man
            .entrants
            .iter()
            .find(|e| e.id == "LOGIC_H105")
            .expect("LOGIC_H105");
        assert!(logic.engine.as_ref().unwrap().contains("LOGIC_H105"));

        let seed = EvalCheckpoint::load_path(&cfg.seed_model).unwrap();
        let cell = EvalCheckpoint::load_path(tmp.join("SEED.json")).unwrap();
        assert_eq!(
            cell.weights.piece[&PieceType::HookMover],
            seed.weights.piece[&PieceType::HookMover]
        );
        assert!((cell.weights.eg_tropism_scale - 1.2).abs() < 1e-6);
        assert!((cell.weights.rank_factor_fast[0] - 0.65).abs() < 1e-5);

        let k100 = EvalCheckpoint::load_path(tmp.join("C2K100A1.json")).unwrap();
        let d50 = EvalCheckpoint::load_path(tmp.join("C2K100A1D50.json")).unwrap();
        let k50 = EvalCheckpoint::load_path(tmp.join("C2K50A1.json")).unwrap();
        let d25 = EvalCheckpoint::load_path(tmp.join("C2K50A1D25.json")).unwrap();
        assert!((k100.weights.two_mover_mob_k - 100.0).abs() < 1e-6);
        assert!((k50.weights.two_mover_mob_k - 50.0).abs() < 1e-6);
        assert_eq!(
            k100.weights.piece[&PieceType::HookMover],
            seed.weights.piece[&PieceType::HookMover]
        );
        assert!(
            (d50.weights.piece[&PieceType::HookMover]
                - (seed.weights.piece[&PieceType::HookMover] - 50.0))
                .abs()
                < 1e-3
        );
        assert!(
            (d25.weights.piece[&PieceType::Peacock]
                - (seed.weights.piece[&PieceType::Peacock] - 25.0))
                .abs()
                < 1e-3
        );

        let _ = fs::remove_dir_all(&tmp);
    }
}
