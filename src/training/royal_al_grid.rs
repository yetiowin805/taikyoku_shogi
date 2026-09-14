//! Reproducible 5×5 A/L experiment, plus six retired weight winners and one old engine.
use crate::eval::{EvalCheckpoint, LastRoyalMode};
use crate::training::history::{load_seed_at, logic_binary_path};
use crate::training::q_rs_grid::{apply_rs_search, build_chassis_weights};
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use serde::Serialize;
use std::{fs, path::Path};

pub const OLD_ENGINE: &str = "LOGIC_PRE_ROYAL_AL";
pub const SOURCE_REV: &str = "ee580c4";
pub const BASE: &str = "BASE_P120H50B75_C2";
pub const RETIRED: [&str; 6] = [
    "AVG_P120_SEED_C2R",
    "C2K100A1D50R",
    "T150_P120_T12_C2R",
    "C2K50A1",
    "SEED",
    "SEEDS2",
];
pub const A_SETTINGS: [(&str, f32, bool); 5] = [
    ("A0", 0., false),
    ("A40", 40., false),
    ("A80", 80., false),
    ("A160", 160., false),
    ("AB80", 80., true),
];
pub const L_SETTINGS: [(&str, LastRoyalMode, f32); 5] = [
    ("L0", LastRoyalMode::Legacy, 0.),
    ("Lold", LastRoyalMode::Legacy, 4000.),
    ("Lflight", LastRoyalMode::VerifiedFlights, 4000.),
    ("Ldefense", LastRoyalMode::ScarceDefenses, 0.),
    ("Lmate", LastRoyalMode::MateProbe, 0.),
];

/// Rebuild the actual August grid from its pinned seed and material revisions.
/// No current mutable seed, raw checkpoint dumps or rounded weight substitutions.
pub fn retired_checkpoint(rev: &str, id: &str) -> Result<EvalCheckpoint, String> {
    let mut cp = load_seed_at(rev, id)?;
    let t150 = load_seed_at("0c0444d", "T150")?;
    let h120 = load_seed_at("fae5573", "H120")?;
    let h105 = load_seed_at("12791cb", "H105")?;
    let original_id = id.strip_prefix("QRS_").unwrap_or(id);
    let (chassis, r, s2) = if let Some(base) = original_id.strip_suffix("S2") {
        (base, false, true)
    } else if let Some(base) = original_id.strip_suffix('R') {
        (base, true, false)
    } else {
        (original_id, false, false)
    };
    cp.weights = build_chassis_weights(
        &cp.weights,
        &t150.weights.piece,
        &h120.weights.piece,
        &h105.weights.piece,
    )
    .into_iter()
    .find(|(name, _)| name == chassis)
    .ok_or_else(|| format!("unknown retired q-rs chassis: {id}"))?
    .1;
    cp.search_defaults = apply_rs_search(&cp.search_defaults, r, false, false, s2);
    Ok(cp)
}

pub fn experimental_cells(base: &EvalCheckpoint) -> Vec<EvalCheckpoint> {
    let mut cells = Vec::with_capacity(25);
    for (a, k, blocked) in A_SETTINGS {
        for (l, mode, flight_k) in L_SETTINGS {
            let mut cp = base.clone();
            cp.name = format!("BASE_C2S2_{a}_{l}");
            cp.search_defaults.q_own_large_only = true;
            cp.weights.two_mover_align_k = k;
            cp.weights.two_mover_align_cap = 5. * k;
            cp.weights.two_mover_align_blocked = blocked;
            cp.weights.lr_flight_k = flight_k;
            cp.weights.last_royal_mode = mode;
            cells.push(cp);
        }
    }
    cells
}

#[derive(Serialize)]
struct GridDescription<'a> {
    schema_version: u32,
    base_id: &'a str,
    source_revision: &'a str,
    shared_changes: [&'a str; 3],
    experimental_ids: Vec<String>,
    historical_weights: [&'a str; 6],
    frozen_reference: &'a str,
    frozen_engine: &'a str,
}

/// Refuse to overwrite a prepared field. Write manifest last; it is the readiness marker.
pub fn generate(out: &Path) -> Result<TourneyManifest, String> {
    if out.exists() {
        return Err(format!("output already exists: {}", out.display()));
    }
    let base = retired_checkpoint(SOURCE_REV, BASE)?;
    let cells = experimental_cells(&base);
    let mut checkpoints: Vec<_> = cells.iter().cloned().map(|cp| (cp, None)).collect();
    for id in RETIRED {
        checkpoints.push((retired_checkpoint(SOURCE_REV, id)?, None));
    }
    checkpoints.push((
        base,
        Some(logic_binary_path(OLD_ENGINE).display().to_string()),
    ));
    fs::create_dir_all(out).map_err(|e| format!("create {}: {e}", out.display()))?;
    let mut entrants = Vec::with_capacity(32);
    for (cp, engine) in checkpoints {
        let model = out.join(format!("{}.json", cp.name));
        cp.save_path(&model)
            .map_err(|e| format!("save {}: {e}", model.display()))?;
        entrants.push(TourneyEntrant {
            id: cp.name,
            model: model.display().to_string(),
            engine,
        });
    }
    let description = GridDescription {
        schema_version: 1,
        base_id: BASE,
        source_revision: SOURCE_REV,
        shared_changes: ["S2", "indexed swap-removal", "aspiration 500"],
        experimental_ids: cells.into_iter().map(|cp| cp.name).collect(),
        historical_weights: RETIRED,
        frozen_reference: BASE,
        frozen_engine: OLD_ENGINE,
    };
    fs::write(
        out.join("grid.json"),
        serde_json::to_vec_pretty(&description).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write grid description: {e}"))?;
    let manifest = TourneyManifest { entrants };
    fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write grid manifest: {e}"))?;
    Ok(manifest)
}

pub fn cli(args: &[String]) -> Result<(), String> {
    let out = match args {
        [] => "models/royal-al-grid",
        [flag, path] if flag == "--out" => path,
        _ => return Err("usage: royal-al-grid [--out DIRECTORY]".into()),
    };
    let manifest = generate(Path::new(out))?;
    println!("Prepared {} agents in {out}/manifest.json; tournament not started. Freeze {OLD_ENGINE} before launch.", manifest.entrants.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    #[test]
    fn full_factorial_is_unique_and_round_trips() {
        let base = retired_checkpoint(SOURCE_REV, BASE).unwrap();
        let cells = experimental_cells(&base);
        assert_eq!(cells.len(), 25);
        let mut policies = HashSet::new();
        for cp in cells {
            let back: EvalCheckpoint =
                serde_json::from_value(serde_json::to_value(&cp).unwrap()).unwrap();
            assert!(back.search_defaults.q_own_large_only);
            assert_eq!(back.weights.piece, base.weights.piece);
            assert_eq!(back.weights.last_royal_mode, cp.weights.last_royal_mode);
            policies.insert(format!(
                "{}/{}/{:?}",
                cp.weights.two_mover_align_k,
                cp.weights.two_mover_align_blocked,
                (cp.weights.last_royal_mode, cp.weights.lr_flight_k)
            ));
        }
        assert_eq!(policies.len(), 25);
    }
    #[test]
    fn grid_preserves_references_and_refuses_overwrite() {
        let dir = std::env::temp_dir().join(format!("royal-al-grid-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let manifest = generate(&dir).unwrap();
        assert_eq!(manifest.entrants.len(), 32);
        assert_eq!(
            manifest
                .entrants
                .iter()
                .filter(|e| e.engine.is_some())
                .count(),
            1
        );
        let frozen = manifest.entrants.iter().find(|e| e.id == BASE).unwrap();
        let cp = EvalCheckpoint::load_path(&frozen.model).unwrap();
        assert!(!cp.search_defaults.q_own_large_only);
        assert_eq!(cp.weights.lr_flight_k, 0.);
        assert_eq!(cp.weights.two_mover_align_k, 0.);
        assert!(generate(&dir).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
}
