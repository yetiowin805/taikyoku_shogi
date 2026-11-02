//! Every-game rank pruning selected by the October 2026 ordering replay.
//! These cluster-robust Wald bounds are empirical, not anytime-valid intervals.
use super::color_model::{variance, Model};
use super::order_neutral::Fit;
use super::tournament::{SlotStatus, TourneyState};
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
pub struct Generation {
    pub members: Vec<String>,
    pub representative: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub policy: &'static str,
    pub z: f64,
    pub minimum_games: usize,
    pub minimum_covered_opponents: usize,
    /// Largest nontrivial SCC, retained for report compatibility.
    pub core: Vec<String>,
    pub minimum_scc_agents: usize,
    pub eligible_components: Vec<Vec<String>>,
    pub candidates: BTreeMap<String, Candidate>,
    pub generations: BTreeMap<String, Generation>,
    /// Ordinary retirement candidates kept eligible by their generation.
    pub retained_by_generation: BTreeSet<String>,
    /// Unknown IDs need explicit entrant metadata to receive cohort protection.
    pub unclassified: BTreeSet<String>,
    pub inactive: BTreeSet<String>,
}

impl Report {
    fn protect_generations(&mut self) {
        for generation in self.generations.values() {
            if self.inactive.remove(&generation.representative) {
                self.retained_by_generation
                    .insert(generation.representative.clone());
            }
        }
    }
}

fn generations(
    state: &TourneyState,
    fit: &Fit,
) -> Result<(BTreeMap<String, Generation>, BTreeSet<String>), String> {
    let mut groups = BTreeMap::<String, Generation>::new();
    let mut unclassified = BTreeSet::new();
    // Use all registered entrants, including previously retired ones. Historical
    // slot-only identities are evidence, but cannot be scheduled representatives.
    for entrant in &state.entrants {
        let Some(label) = super::generations::label(entrant)? else {
            unclassified.insert(entrant.id.clone());
            continue;
        };
        let rating = fit
            .ratings
            .get(&entrant.id)
            .filter(|r| r.is_finite())
            .ok_or_else(|| format!("missing/nonfinite generation rating: {}", entrant.id))?;
        let group = groups.entry(label.into()).or_insert_with(|| Generation {
            members: Vec::new(),
            representative: entrant.id.clone(),
        });
        let best = fit.ratings[&group.representative];
        if *rating > best || (*rating == best && entrant.id < group.representative) {
            group.representative.clone_from(&entrant.id);
        }
        group.members.push(entrant.id.clone());
    }
    for group in groups.values_mut() {
        group.members.sort();
    }
    Ok((groups, unclassified))
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
    // Retain the largest SCC label for older consumers. Retirement is evaluated
    // independently in every SCC with at least nine agents.
    let core = fit
        .components
        .iter()
        .filter(|g| g.len() > 1)
        .min_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)))
        .cloned()
        .unwrap_or_default();
    let (generations, unclassified) = generations(state, fit)?;
    let mut report = Report {
        policy: "rank8-three50-z1.96-g20o6-scc9-generations-v4",
        z: Z,
        minimum_games: 20,
        minimum_covered_opponents: 6,
        core,
        minimum_scc_agents: 9,
        eligible_components: fit
            .components
            .iter()
            .filter(|g| g.len() >= 9)
            .cloned()
            .collect(),
        candidates: BTreeMap::new(),
        generations,
        retained_by_generation: BTreeSet::new(),
        unclassified,
        inactive: BTreeSet::new(),
    };
    if report.eligible_components.is_empty() {
        report.protect_generations();
        return Ok(report);
    }
    let mut model = Model::new(state, fit)?;
    model.set_from_fit(fit);
    let cov = model.covariance()?;
    let meat = model.cluster_meat(state);
    let robust = multiply(&multiply(&cov, &meat), &cov);
    for group in &report.eligible_components {
        let n = group.len();
        let ratings = group
            .iter()
            .map(|id| fit.ratings.get(id).copied().ok_or("missing SCC rating"))
            .collect::<Result<Vec<_>, _>>()?;
        let index: BTreeMap<_, _> = group.iter().enumerate().map(|(i, id)| (id, i)).collect();
        let mut opponents = vec![BTreeMap::<&str, usize>::new(); n];
        for slot in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
            // Cross-SCC games remain historical rating evidence, but cannot
            // satisfy this SCC's retirement exposure/coverage requirements.
            let (Some(&i), Some(&j)) = (index.get(&slot.model_a), index.get(&slot.model_b)) else {
                continue;
            };
            *opponents[i].entry(&slot.model_b).or_default() += 1;
            *opponents[j].entry(&slot.model_a).or_default() += 1;
        }
        for i in 0..n {
            let lower = (0..n)
                .filter(|&j| j != i)
                .map(|j| {
                    let contrast = model.contrast(&group[i], &group[j]);
                    let model_variance = variance(&cov, &contrast);
                    let block_variance = variance(&robust, &contrast);
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
                report.inactive.insert(group[i].clone());
            }
            report.candidates.insert(group[i].clone(), candidate);
        }
    }
    report.protect_generations();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{
        build_schedule, ensure_ratings, SlotStartMode, TourneyConfig, TourneyEntrant,
        TourneyFormat, TourneySlot,
    };

    fn fixture() -> TourneyState {
        let mut state = build_schedule(&TourneyConfig {
            format: TourneyFormat::RoundRobin,
            entrants: (0..11)
                .map(|i| TourneyEntrant {
                    id: format!("agent{i:02}"),
                    model: "unused.json".into(),
                    engine: None,
                    generation: None,
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
    fn generation_leader_is_restored_after_pruning_and_changes_with_results() {
        let mut state = fixture();
        // Both cohort members are convincingly weaker than the other eight.
        for slot in &mut state.slots {
            if slot.model_b == "agent08" {
                slot.score_a = Some(f64::from(slot.id % 20 < 18));
            }
        }
        let baseline = super::super::order_neutral::from_state(&state);
        assert_eq!(
            baseline.retirement.as_ref().unwrap().inactive,
            BTreeSet::from(["agent08".into(), "agent09".into()])
        );
        for entrant in &mut state.entrants[8..10] {
            entrant.generation = Some("historical".into());
        }
        state.retired = BTreeSet::from(["agent08".into(), "agent09".into()]);
        let slots = serde_json::to_value(&state.slots).unwrap();
        ensure_ratings(&mut state);
        assert_eq!(state.retired, BTreeSet::from(["agent09".into()]));
        assert_eq!(serde_json::to_value(&state.slots).unwrap(), slots);
        let fit = super::super::order_neutral::from_state(&state);
        let report = fit.retirement.as_ref().unwrap();
        assert_eq!(report.generations["historical"].representative, "agent08");
        assert_eq!(
            report.retained_by_generation,
            BTreeSet::from(["agent08".into()])
        );
        // Protection changes the eligible center, never the fitted differences.
        for id in fit.ratings.keys() {
            if id == "agent10" {
                continue; // Disconnected newcomers have an independent center.
            }
            assert!(
                ((fit.ratings[id] - fit.ratings["agent00"])
                    - (baseline.ratings[id] - baseline.ratings["agent00"]))
                    .abs()
                    < 1e-9
            );
        }
        // Exchange the two agents' evidence. The previously retired winner
        // returns and the previous representative can now retire.
        for slot in &mut state.slots {
            for id in [&mut slot.model_a, &mut slot.model_b] {
                match id.as_str() {
                    "agent08" => *id = "agent09".into(),
                    "agent09" => *id = "agent08".into(),
                    _ => {}
                }
            }
        }
        ensure_ratings(&mut state);
        assert_eq!(state.retired, BTreeSet::from(["agent08".into()]));
        let before = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        assert_eq!(before.generations["historical"].representative, "agent09");
        state.entrants.reverse();
        state.slots.reverse();
        let after = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
    }

    #[test]
    fn former_generation_labels_merge_before_representative_selection() {
        for (weak, strong, combined) in [
            ("v2", "v3", "v2_v3"),
            ("v4_v4.5", "v5", "v4_v5"),
            ("search_wave_1", "search_wave_2", "search"),
        ] {
            let mut state = fixture();
            state.entrants[9].generation = Some(weak.into());
            state.entrants[0].generation = Some(strong.into());
            ensure_ratings(&mut state);
            assert!(state.retired.contains("agent09"));
            let report = super::super::order_neutral::from_state(&state)
                .retirement
                .unwrap();
            assert_eq!(report.generations.len(), 1);
            assert_eq!(report.generations[combined].representative, "agent00");
            assert_eq!(report.generations[combined].members, ["agent00", "agent09"]);
            assert!(report.retained_by_generation.is_empty());
        }
    }

    #[test]
    fn generation_report_in_small_disconnected_field_has_stable_ties() {
        let mut state = fixture();
        state.slots.clear();
        for entrant in &mut state.entrants[..2] {
            entrant.generation = Some("new_generation".into());
        }
        state.retired = state.entrants.iter().map(|e| e.id.clone()).collect();
        state.entrants.reverse();
        ensure_ratings(&mut state);
        assert!(state.retired.is_empty());
        let report = super::super::order_neutral::from_state(&state)
            .retirement
            .unwrap();
        assert!(report.core.is_empty());
        assert_eq!(
            report.generations["new_generation"].representative,
            "agent00"
        );
        assert_eq!(
            report.generations["new_generation"].members,
            ["agent00", "agent01"]
        );
        assert_eq!(report.unclassified.len(), 9);
    }

    fn nine_agent_scc() -> TourneyState {
        let mut state = fixture();
        state
            .entrants
            .retain(|e| e.id != "agent08" && e.id != "agent10");
        state
            .slots
            .retain(|s| s.model_a != "agent08" && s.model_b != "agent08");
        state
    }

    #[test]
    fn every_nine_agent_scc_can_retire_without_a_majority() {
        let mut state = nine_agent_scc();
        let mut second = state.clone();
        for entrant in &mut second.entrants {
            entrant.id = entrant.id.replace("agent", "other");
        }
        for slot in &mut second.slots {
            slot.model_a = slot.model_a.replace("agent", "other");
            slot.model_b = slot.model_b.replace("agent", "other");
            slot.id += state.slots.len();
        }
        state.entrants.extend(second.entrants);
        state.slots.extend(second.slots);
        // Neither 9-agent SCC is more than half this 19-agent field.
        state.entrants.push(TourneyEntrant {
            id: "unplayed".into(),
            model: "unused.json".into(),
            engine: None,
            generation: None,
        });
        let fit = super::super::order_neutral::from_state(&state);
        let report = fit.retirement.unwrap();
        assert_eq!(report.minimum_scc_agents, 9);
        assert_eq!(report.eligible_components.len(), 2);
        assert_eq!(report.candidates.len(), 18);
        assert_eq!(
            report.inactive,
            BTreeSet::from(["agent09".into(), "other09".into()])
        );
        assert!(!report.candidates.contains_key("unplayed"));
    }

    #[test]
    fn fewer_than_nine_agents_cannot_retire() {
        let mut state = nine_agent_scc();
        state.entrants.retain(|e| e.id != "agent07");
        state
            .slots
            .retain(|s| s.model_a != "agent07" && s.model_b != "agent07");
        state.retired.insert("agent09".into());
        ensure_ratings(&mut state);
        let fit = super::super::order_neutral::from_state(&state);
        let report = fit.retirement.unwrap();
        assert_eq!(report.core.len(), 8);
        assert!(report.eligible_components.is_empty());
        assert!(report.candidates.is_empty());
        assert!(state.retired.is_empty());
    }

    #[test]
    fn outside_scc_games_cannot_satisfy_retirement_coverage() {
        let mut state = fixture();
        // Keep one win each way against every core opponent: 18 SCC games.
        state
            .slots
            .retain(|slot| slot.model_b != "agent09" || matches!(slot.id % 20, 0 | 18));
        for _ in 0..20 {
            let mut slot = state.slots[0].clone();
            slot.id = state.slots.iter().map(|s| s.id).max().unwrap() + 1;
            slot.model_a = "agent09".into();
            slot.model_b = "agent10".into();
            slot.score_a = Some(0.);
            state.slots.push(slot);
        }
        let fit = super::super::order_neutral::from_state(&state);
        let report = fit.retirement.unwrap();
        let candidate = &report.candidates["agent09"];
        assert_eq!(candidate.games, 18);
        assert_eq!(candidate.covered_opponents, 9);
        assert!(!candidate.coverage_met);
        assert!(!report.inactive.contains("agent09"));
        assert!(!report.candidates.contains_key("agent10"));
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
}
