//! Every-game rank pruning selected by the October 2026 ordering replay.
//! These cluster-robust Wald bounds are empirical, not anytime-valid intervals.
use super::order_neutral::{solve, Fit};
use super::tournament::{SlotStartMode, SlotStatus, TourneyState};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const Z: f64 = 1.959963985;
const SCALE: f64 = 400.0 / std::f64::consts::LN_10;

#[derive(Debug, Serialize)]
pub struct Candidate {
    pub games: usize,
    pub covered_opponents: usize,
    pub stronger_rivals: usize,
    pub rivals_ahead_by_50: usize,
    pub coverage_met: bool,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub policy: &'static str,
    pub z: f64,
    pub minimum_games: usize,
    pub minimum_covered_opponents: usize,
    pub core: Vec<String>,
    pub candidates: BTreeMap<String, Candidate>,
    pub inactive: BTreeSet<String>,
}

fn covariance(count: &[Vec<f64>], ratings: &[f64]) -> Result<Vec<Vec<f64>>, String> {
    let n = count.len();
    let mut information = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let p = 1.0 / (1.0 + (-(ratings[i] - ratings[j]) / SCALE).exp());
            let v = count[i][j] * p * (1.0 - p);
            information[i][i] += v;
            information[j][j] += v;
            information[i][j] -= v;
            information[j][i] -= v;
        }
    }
    // Invert on the centered subspace. The added constant direction is removed
    // afterwards; pairwise contrast variances are independent of the anchor.
    let offset = 1.0 / n as f64;
    for row in &mut information {
        for v in row {
            *v += offset;
        }
    }
    let mut cov = vec![vec![0.0; n]; n];
    for j in 0..n {
        let mut unit = vec![0.0; n];
        unit[j] = 1.0;
        let column = solve(information.clone(), unit).ok_or("singular retirement information")?;
        for (row, v) in cov.iter_mut().zip(column) {
            row[j] = v - offset;
        }
    }
    Ok(cov)
}

fn multiply(a: &[Vec<f64>], b: &[Vec<f64>]) -> Vec<Vec<f64>> {
    a.iter()
        .map(|row| {
            (0..b.len())
                .map(|j| row.iter().enumerate().map(|(k, v)| v * b[k][j]).sum())
                .collect()
        })
        .collect()
}

fn classify(games: usize, covered: usize, lower: impl Iterator<Item = f64>) -> (Candidate, bool) {
    let mut candidate = Candidate {
        games,
        covered_opponents: covered,
        stronger_rivals: 0,
        rivals_ahead_by_50: 0,
        coverage_met: games >= 20 && covered >= 6,
    };
    for gap in lower {
        candidate.stronger_rivals += usize::from(gap > 0.0);
        candidate.rivals_ahead_by_50 += usize::from(gap > 50.0);
    }
    let inactive = candidate.coverage_met
        && candidate.stronger_rivals >= 8
        && candidate.rivals_ahead_by_50 >= 3;
    (candidate, inactive)
}

pub(super) fn evaluate(state: &TourneyState, fit: &Fit) -> Result<Report, String> {
    // Match the replay's largest SCC, including deterministic equal-size ties.
    // New/separated agents stay eligible for connectivity repair.
    let core = fit
        .components
        .iter()
        .filter(|g| g.len() > 1)
        .min_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)))
        .cloned()
        .unwrap_or_default();
    let mut report = Report {
        policy: "rank8-three50-z1.96-g20o6-v1",
        z: Z,
        minimum_games: 20,
        minimum_covered_opponents: 6,
        core,
        candidates: BTreeMap::new(),
        inactive: BTreeSet::new(),
    };
    let n = report.core.len();
    if n < 9 {
        return Ok(report);
    }
    let ratings = report
        .core
        .iter()
        .map(|id| fit.ratings.get(id).copied().ok_or("missing core rating"))
        .collect::<Result<Vec<_>, _>>()?;
    let index: BTreeMap<_, _> = report
        .core
        .iter()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let mut count = vec![vec![0.0; n]; n];
    let mut opponents = vec![BTreeMap::<&str, usize>::new(); n];
    let mut blocks = BTreeMap::<(usize, usize, u64, bool), (usize, f64)>::new();
    let mut total_games = 0;
    for slot in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
        total_games += 1;
        let left = index.get(&slot.model_a).copied();
        let right = index.get(&slot.model_b).copied();
        if let Some(i) = left {
            *opponents[i].entry(&slot.model_b).or_default() += 1;
        }
        if let Some(j) = right {
            *opponents[j].entry(&slot.model_a).or_default() += 1;
        }
        if let (Some(i), Some(j)) = (left, right) {
            let score = slot.score_a.ok_or("completed slot missing score")?;
            if i == j || ![0.0, 0.5, 1.0].contains(&score) {
                return Err("invalid completed result".into());
            }
            count[i][j] += 1.0;
            count[j][i] += 1.0;
            let (a, b, score) = if i < j {
                (i, j, score)
            } else {
                (j, i, 1.0 - score)
            };
            let block = blocks
                .entry((
                    a,
                    b,
                    slot.start_seed,
                    slot.start_mode == SlotStartMode::Opening,
                ))
                .or_default();
            block.0 += 1;
            block.1 += score;
        }
    }
    let cov = covariance(&count, &ratings)?;
    let mut meat = vec![vec![0.0; n]; n];
    for (&(i, j, _, _), &(games, score)) in &blocks {
        let p = 1.0 / (1.0 + (-(ratings[i] - ratings[j]) / SCALE).exp());
        let v = (score - games as f64 * p).powi(2);
        meat[i][i] += v;
        meat[j][j] += v;
        meat[i][j] -= v;
        meat[j][i] -= v;
    }
    if blocks.len() > 1 && total_games > n - 1 {
        let correction = blocks.len() as f64 / (blocks.len() - 1) as f64 * (total_games - 1) as f64
            / (total_games - n + 1) as f64;
        for row in &mut meat {
            for v in row {
                *v *= correction;
            }
        }
    }
    let robust = multiply(&multiply(&cov, &meat), &cov);
    for i in 0..n {
        let lower = (0..n)
            .filter(|&j| j != i)
            .map(|j| {
                let model_variance = cov[i][i] + cov[j][j] - 2.0 * cov[i][j];
                let block_variance = robust[i][i] + robust[j][j] - 2.0 * robust[i][j];
                let se = SCALE * model_variance.max(block_variance).max(0.0).sqrt();
                ratings[j] - ratings[i] - Z * se
            })
            .collect::<Vec<_>>();
        if lower.iter().any(|v| !v.is_finite()) {
            return Err("nonfinite retirement bound".into());
        }
        let games = opponents[i].values().sum();
        let covered = opponents[i].values().filter(|&&v| v >= 2).count();
        let (candidate, inactive) = classify(games, covered, lower.into_iter());
        if inactive {
            report.inactive.insert(report.core[i].clone());
        }
        report.candidates.insert(report.core[i].clone(), candidate);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{
        build_schedule, ensure_ratings, TourneyConfig, TourneyEntrant, TourneyFormat, TourneySlot,
    };

    fn fixture() -> TourneyState {
        let mut state = build_schedule(&TourneyConfig {
            format: TourneyFormat::RoundRobin,
            entrants: (0..11)
                .map(|i| TourneyEntrant {
                    id: format!("agent{i:02}"),
                    model: "unused.json".into(),
                    engine: None,
                })
                .collect(),
            ..Default::default()
        });
        state.order_neutral_only = true;
        state.slots.clear();
        for i in 0..10 {
            for j in i + 1..10 {
                for leg in 0..20 {
                    let id = state.slots.len();
                    state.slots.push(TourneySlot {
                        id,
                        model_a: format!("agent{i:02}"),
                        model_b: format!("agent{j:02}"),
                        start_seed: (id / 2) as u64,
                        a_is_black: leg % 2 == 0,
                        status: SlotStatus::Done,
                        game_path: None,
                        score_a: Some(if j == 9 { f64::from(leg < 18) } else { 0.5 }),
                        round: 0,
                        start_mode: SlotStartMode::Opening,
                    });
                }
            }
        }
        state
    }

    #[test]
    fn full_state_prunes_weak_core_agent_and_preserves_newcomer() {
        let mut state = fixture();
        state.retired.insert("agent10".into());
        let slots = serde_json::to_value(&state.slots).unwrap();
        ensure_ratings(&mut state);
        assert_eq!(state.retired, BTreeSet::from(["agent09".into()]));
        assert_eq!(serde_json::to_value(&state.slots).unwrap(), slots);
        let report = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        assert_eq!(report.candidates["agent09"].games, 180);
        assert_eq!(report.candidates["agent09"].covered_opponents, 9);
        assert!(!report.candidates.contains_key("agent10"));

        // A newly completed balanced result changes eligibility immediately;
        // past decisions cannot prevent reactivation under the current evidence.
        for slot in &mut state.slots {
            slot.score_a = Some(0.5);
        }
        ensure_ratings(&mut state);
        assert!(state.retired.is_empty());
    }

    #[test]
    fn orientation_order_and_existing_retirement_do_not_change_decisions() {
        let mut state = fixture();
        let before = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        state.slots.reverse();
        state.retired = state.entrants.iter().map(|e| e.id.clone()).collect();
        for slot in &mut state.slots {
            std::mem::swap(&mut slot.model_a, &mut slot.model_b);
            slot.score_a = slot.score_a.map(|s| 1.0 - s);
            slot.a_is_black = !slot.a_is_black;
        }
        let after = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
    }

    #[test]
    fn coverage_rank_and_competitiveness_boundaries() {
        let bounds = [51.0, 51.0, 51.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        assert!(classify(20, 6, bounds.into_iter()).1);
        assert!(!classify(19, 6, bounds.into_iter()).1);
        assert!(!classify(20, 5, bounds.into_iter()).1);
        assert!(!classify(20, 6, bounds[..7].iter().copied()).1);
        let mut boundary = bounds;
        boundary[0] = 50.0;
        assert!(!classify(20, 6, boundary.into_iter()).1);
        boundary = bounds;
        boundary[7] = 0.0;
        assert!(!classify(20, 6, boundary.into_iter()).1);
    }

    #[test]
    fn information_contrast_matches_binomial_variance() {
        let c = covariance(&[vec![0.0, 100.0], vec![100.0, 0.0]], &[1500.0, 1500.0]).unwrap();
        let variance = c[0][0] + c[1][1] - 2.0 * c[0][1];
        assert!((variance - 0.04).abs() < 1e-10);
    }
}
