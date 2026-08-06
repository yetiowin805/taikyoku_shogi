//! Sample ±10% perturbations of high-impact eval parameters for Swiss tourneys.

use crate::eval::{EvalCheckpoint, EvalWeights, ALL_PIECE_TYPES};
use crate::piece::PieceType;
use crate::training::tournament::{TourneyEntrant, TourneyManifest};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_N_SAMPLES: usize = 31;
pub const DEFAULT_RNG_SEED: u64 = 1;
pub const DEFAULT_OUT_DIR: &str = "models/scale-sample";
pub const DEFAULT_SEED_MODEL: &str = "models/ab-seed.json";

const MULTS: [f32; 3] = [0.9, 1.0, 1.1];
const MAX_DRAW_ATTEMPTS: usize = 100_000;

pub use crate::eval::is_big_piece;

/// Ordered list of big piece types (stable for RNG / samples.json).
pub fn big_piece_types() -> Vec<PieceType> {
    ALL_PIECE_TYPES
        .iter()
        .copied()
        .filter(|pt| is_big_piece(*pt))
        .collect()
}

#[derive(Debug, Clone)]
pub struct ScaleSampleConfig {
    pub seed_model: PathBuf,
    pub out_dir: PathBuf,
    pub n_samples: usize,
    pub rng_seed: u64,
}

impl Default for ScaleSampleConfig {
    fn default() -> Self {
        Self {
            seed_model: PathBuf::from(DEFAULT_SEED_MODEL),
            out_dir: PathBuf::from(DEFAULT_OUT_DIR),
            n_samples: DEFAULT_N_SAMPLES,
            rng_seed: DEFAULT_RNG_SEED,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecord {
    pub id: String,
    pub model: String,
    /// Multipliers aligned with [`param_names`] (length 17).
    pub multipliers: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplesFile {
    pub rng_seed: u64,
    pub seed_model: String,
    pub param_names: Vec<String>,
    pub samples: Vec<SampleRecord>,
}

pub fn param_names() -> Vec<String> {
    let mut names: Vec<String> = big_piece_types()
        .into_iter()
        .map(|pt| format!("{:?}", pt))
        .collect();
    names.push("King".into());
    names.push("royal_bonus_2".into());
    names
}

fn link_royal_bonus_3(weights: &mut EvalWeights) {
    let bonus2 = weights.royal_bonus(2);
    let bonus3 = ((bonus2 as f64) * 6.0 / 5.0).round() as i32;
    let mut table = weights.royal_bonus_by_count.clone();
    while table.len() < 4 {
        table.push(0);
    }
    table[3] = bonus3;
    weights.royal_bonus_by_count = table;
}

fn apply_multipliers(base: &EvalWeights, mults: &[f32]) -> Result<EvalWeights, String> {
    let pieces = big_piece_types();
    let expected = pieces.len() + 2; // + King + royal_bonus_2
    if mults.len() != expected {
        return Err(format!(
            "expected {} multipliers, got {}",
            expected,
            mults.len()
        ));
    }
    let mut w = base.clone();
    for (i, pt) in pieces.iter().enumerate() {
        let v = base.piece_value(*pt) * mults[i];
        w.piece.insert(*pt, v);
    }
    let king_m = mults[pieces.len()];
    w.piece
        .insert(PieceType::King, base.piece_value(PieceType::King) * king_m);
    let rb2 = ((base.royal_bonus(2) as f32) * mults[pieces.len() + 1]).round() as i32;
    let mut table = w.royal_bonus_by_count.clone();
    while table.len() < 4 {
        table.push(0);
    }
    table[2] = rb2;
    w.royal_bonus_by_count = table;
    link_royal_bonus_3(&mut w);
    w.rebuild_piece_value_table();
    Ok(w)
}

fn draw_multipliers(rng: &mut StdRng, n_params: usize) -> Vec<f32> {
    (0..n_params)
        .map(|_| MULTS[rng.gen_range(0..MULTS.len())])
        .collect()
}

fn all_ones(m: &[f32]) -> bool {
    m.iter().all(|x| (*x - 1.0).abs() < 1e-6)
}

fn mult_key(m: &[f32]) -> Vec<i32> {
    // Stable key in tenths: 0.9 → 9, 1.0 → 10, 1.1 → 11
    m.iter().map(|x| (x * 10.0).round() as i32).collect()
}

fn write_sample(
    base_cp: &EvalCheckpoint,
    base: &EvalWeights,
    out_dir: &Path,
    id: &str,
    mults: &[f32],
) -> Result<SampleRecord, String> {
    let weights = apply_multipliers(base, mults)?;
    let mut cp = base_cp.clone();
    cp.name = id.to_string();
    cp.weights = weights;
    let model_path = out_dir.join(format!("{id}.json"));
    cp.save_path(&model_path)
        .map_err(|e| format!("save {}: {e}", model_path.display()))?;
    Ok(SampleRecord {
        id: id.to_string(),
        model: model_path.display().to_string(),
        multipliers: mults.to_vec(),
    })
}

/// Copy seed + write `n_samples` unique models (all−10%, all+10%, then random) and a manifest.
///
/// `n_samples` is the number of non-seed entrants (default 31 → 32 total with seed).
/// The first two are fixed uniform ±10%; the rest are random unique draws.
pub fn run_scale_sample(cfg: &ScaleSampleConfig) -> Result<(TourneyManifest, SamplesFile), String> {
    if cfg.n_samples < 2 {
        return Err("n_samples must be >= 2 (need slots for all_m10 and all_p10)".into());
    }
    if !cfg.seed_model.is_file() {
        return Err(format!(
            "missing seed model {} (copy existing checkpoint; do not regenerate)",
            cfg.seed_model.display()
        ));
    }
    let base_cp = EvalCheckpoint::load_path(&cfg.seed_model)?;
    let base = &base_cp.weights;
    let names = param_names();
    let n_params = names.len();
    let pieces = big_piece_types();
    if pieces.len() != 15 {
        return Err(format!(
            "expected 15 big pieces, found {} ({:?})",
            pieces.len(),
            pieces
        ));
    }
    if n_params != 17 {
        return Err(format!("expected 17 params, got {n_params}"));
    }

    fs::create_dir_all(&cfg.out_dir)
        .map_err(|e| format!("create {}: {e}", cfg.out_dir.display()))?;

    let seed_out = cfg.out_dir.join("seed.json");
    // Copy bytes so we do not rewrite/regenerate seed weights.
    fs::copy(&cfg.seed_model, &seed_out).map_err(|e| {
        format!(
            "copy {} → {}: {e}",
            cfg.seed_model.display(),
            seed_out.display()
        )
    })?;

    let mut seen: HashSet<Vec<i32>> = HashSet::new();
    seen.insert(mult_key(&vec![1.0f32; n_params])); // reserve all-ones = seed

    let mut samples = Vec::with_capacity(cfg.n_samples);

    // Fixed anchors before random fills.
    let fixed = [
        ("all_m10", vec![0.9f32; n_params]),
        ("all_p10", vec![1.1f32; n_params]),
    ];
    for (id, mults) in &fixed {
        let key = mult_key(mults);
        if !seen.insert(key) {
            return Err(format!("fixed sample {id} collided with seed/prior"));
        }
        samples.push(write_sample(&base_cp, base, &cfg.out_dir, id, mults)?);
    }

    let mut rng = StdRng::seed_from_u64(cfg.rng_seed);
    let mut attempts = 0usize;
    let mut random_idx = 0usize;
    while samples.len() < cfg.n_samples {
        attempts += 1;
        if attempts > MAX_DRAW_ATTEMPTS {
            return Err(format!(
                "failed to draw {} unique samples after {MAX_DRAW_ATTEMPTS} attempts",
                cfg.n_samples
            ));
        }
        let mults = draw_multipliers(&mut rng, n_params);
        if all_ones(&mults) {
            continue;
        }
        let key = mult_key(&mults);
        if !seen.insert(key) {
            continue;
        }
        random_idx += 1;
        let id = format!("R{:02}", random_idx);
        samples.push(write_sample(
            &base_cp,
            base,
            &cfg.out_dir,
            &id,
            &mults,
        )?);
    }

    let samples_file = SamplesFile {
        rng_seed: cfg.rng_seed,
        seed_model: cfg.seed_model.display().to_string(),
        param_names: names,
        samples: samples.clone(),
    };
    let samples_path = cfg.out_dir.join("samples.json");
    fs::write(
        &samples_path,
        serde_json::to_string_pretty(&samples_file).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write {}: {e}", samples_path.display()))?;

    let mut entrants = vec![TourneyEntrant {
        id: "seed".into(),
        model: seed_out.display().to_string(),
    }];
    for s in &samples {
        entrants.push(TourneyEntrant {
            id: s.id.clone(),
            model: s.model.clone(),
        });
    }
    let manifest = TourneyManifest { entrants };
    let manifest_path = cfg.out_dir.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write {}: {e}", manifest_path.display()))?;

    Ok((manifest, samples_file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_piece_count_is_fifteen() {
        let pts = big_piece_types();
        assert_eq!(pts.len(), 15, "{pts:?}");
        assert!(!pts.contains(&PieceType::King));
        assert!(!pts.contains(&PieceType::WoodenDove));
        assert!(pts.contains(&PieceType::GreatGeneral));
        assert!(pts.contains(&PieceType::HookMover));
        assert!(pts.contains(&PieceType::FreeEagle));
        assert!(pts.contains(&PieceType::Lion));
    }

    #[test]
    fn param_names_seventeen() {
        assert_eq!(param_names().len(), 17);
    }

    #[test]
    fn royal_3_linked_to_six_fifths() {
        let mut w = EvalWeights::seed();
        w.royal_bonus_by_count = vec![0, 0, 5000, 0];
        link_royal_bonus_3(&mut w);
        assert_eq!(w.royal_bonus(3), 6000);
        w.royal_bonus_by_count[2] = 4000;
        link_royal_bonus_3(&mut w);
        assert_eq!(w.royal_bonus(3), 4800);
    }

    #[test]
    fn sample_rejects_seed_and_dupes() {
        let dir = std::env::temp_dir().join(format!(
            "taikyoku-scale-sample-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let seed_path = dir.join("ab-seed.json");
        EvalCheckpoint::seed("ab-seed")
            .save_path(&seed_path)
            .unwrap();

        let out = dir.join("out");
        let cfg = ScaleSampleConfig {
            seed_model: seed_path.clone(),
            out_dir: out.clone(),
            n_samples: 8,
            rng_seed: 42,
        };
        let (man, samples) = run_scale_sample(&cfg).expect("sample");
        assert_eq!(man.entrants.len(), 9); // seed + 8
        assert_eq!(samples.samples.len(), 8);
        assert_eq!(samples.samples[0].id, "all_m10");
        assert_eq!(samples.samples[1].id, "all_p10");
        assert!(samples.samples[0]
            .multipliers
            .iter()
            .all(|m| (*m - 0.9).abs() < 1e-6));
        assert!(samples.samples[1]
            .multipliers
            .iter()
            .all(|m| (*m - 1.1).abs() < 1e-6));
        assert!(out.join("seed.json").is_file());

        let mut keys = HashSet::new();
        keys.insert(mult_key(&vec![1.0f32; 17]));
        for s in &samples.samples {
            assert!(!all_ones(&s.multipliers));
            assert!(keys.insert(mult_key(&s.multipliers)));
            for m in &s.multipliers {
                assert!(MULTS.iter().any(|x| (*x - *m).abs() < 1e-6));
            }
            let cp = EvalCheckpoint::load_path(&s.model).unwrap();
            let b2 = cp.weights.royal_bonus(2);
            let b3 = cp.weights.royal_bonus(3);
            assert_eq!(b3, ((b2 as f64) * 6.0 / 5.0).round() as i32);
        }

        // Seed file is a byte copy of the source checkpoint.
        let a = fs::read(&seed_path).unwrap();
        let b = fs::read(out.join("seed.json")).unwrap();
        assert_eq!(a, b);

        let _ = fs::remove_dir_all(&dir);
    }
}
