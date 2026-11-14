//! Evidence-gated counter tests within the existing exploration budget.
//!
//! The conditional Gaussian/Laplace screen follows the champion-counter model:
//! a 100-Elo zero-centred matchup prior, real results only, and uncertainty in the
//! baseline/benchmark contrast. These are empirical scheduling probabilities,
//! not calibrated sequential significance tests or replacements for ratings.
use super::{model, normal_cdf, Posterior, SCALE};
use crate::training::generations;
use crate::training::tournament::{SlotStartMode, SlotStatus, TourneyState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const MIN_PAIRS: usize = 4;
const MIN_OPPONENTS: usize = 3;
const TEST_PROBABILITY: f64 = 0.25;
const PROTECT_PROBABILITY: f64 = 0.90;
const MARGIN_ELO: f64 = 50.;
const PRIOR_SD_ELO: f64 = 100.;
const FRESH_PAIRS: usize = 4;
const PAIRS_PER_PROBE: usize = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    /// Counts start at zero on upgrade; old pair history creates no probe debt.
    pub scheduled_pairs: usize,
    pub probe_pairs: usize,
    pub reports: Vec<Report>,
    pub admissions: Vec<Admission>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Admission {
    pub agent: String,
    pub generation: String,
    /// Coverage when first eligible. Subsequent ordinary games of any origin
    /// supply confirmation; no slot IDs, batches, or special result classes.
    pub initial_pairs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub agent: String,
    pub generation: String,
    pub benchmark: String,
    pub panel: Vec<String>,
    pub games: usize,
    pub completed_pairs: usize,
    pub paired_opponents: usize,
    pub probability: f64,
    /// Relative to the generation leader; positive means stronger.
    pub margin_elo: f64,
    pub sd_elo: f64,
    pub coverage_met: bool,
    pub eligible: bool,
    pub protected: bool,
    pub fresh_pairs: usize,
}

struct Game {
    opponent: usize,
    score: f64,
    black: bool,
    seed: u64,
    opening: bool,
}

fn pair_coverage(games: &[Game]) -> (usize, usize) {
    let mut blocks = BTreeMap::<(usize, u64, bool), [usize; 2]>::new();
    for g in games {
        blocks.entry((g.opponent, g.seed, g.opening)).or_default()[usize::from(g.black)] += 1;
    }
    let mut opponents = BTreeSet::new();
    let mut pairs = 0;
    for ((opponent, _, _), counts) in blocks {
        let n = counts[0].min(counts[1]);
        if n > 0 {
            opponents.insert(opponent);
            pairs += n;
        }
    }
    (pairs, opponents.len())
}

fn estimate(p: &Posterior, agent: usize, benchmark: usize, games: &[Game]) -> (f64, f64, f64) {
    let precision = (SCALE / PRIOR_SD_ELO).powi(2);
    let mut delta = 0.;
    let mut h = precision;
    for _ in 0..40 {
        let mut gradient = delta * precision;
        h = precision;
        for g in games {
            let offset = p.mean[agent] - p.mean[g.opponent]
                + if g.black {
                    p.mean[p.names.len()]
                } else {
                    -p.mean[p.names.len()]
                };
            let prob = model::sigmoid(offset + delta);
            gradient += prob - g.score;
            h += prob * (1. - prob);
        }
        let step = gradient / h;
        delta -= step;
        if step.abs() < 1e-9 {
            break;
        }
    }
    let baseline_variance = (p.covariance[agent][agent] + p.covariance[benchmark][benchmark]
        - 2. * p.covariance[agent][benchmark])
        .max(0.);
    let sd = (1. / h + baseline_variance).sqrt();
    let margin = p.mean[agent] + delta - p.mean[benchmark];
    (
        normal_cdf((margin + MARGIN_ELO / SCALE) / sd),
        margin * SCALE,
        sd * SCALE,
    )
}

pub(super) fn refresh(
    state: &mut TourneyState,
    p: &Posterior,
    ratings: &BTreeMap<String, f64>,
) -> Result<Vec<(String, String)>, String> {
    let index: BTreeMap<_, _> = p
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut labels = BTreeMap::new();
    for entrant in &state.entrants {
        if let Some(label) = generations::label(entrant)? {
            labels.insert(entrant.id.clone(), label.to_owned());
        }
    }
    let mut ranked: Vec<_> = state.entrants.iter().map(|e| e.id.clone()).collect();
    ranked.sort_by(|a, b| ratings[b].total_cmp(&ratings[a]).then(a.cmp(b)));
    let targets: BTreeSet<_> = ranked
        .iter()
        .take(5)
        .filter_map(|n| labels.get(n))
        .cloned()
        .collect();
    let graph = super::Graph::new(state, &p.names, false);
    let mut games = BTreeMap::<(String, String), Vec<Game>>::new();
    for s in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
        let score = s.score_a.ok_or("completed matchup slot missing score")?;
        for (agent, opponent, score, black) in [
            (&s.model_a, &s.model_b, score, s.a_is_black),
            (&s.model_b, &s.model_a, 1. - score, !s.a_is_black),
        ] {
            let Some(generation) = labels.get(opponent).filter(|g| targets.contains(*g)) else {
                continue;
            };
            if graph.component[index[agent.as_str()]] != graph.component[index[opponent.as_str()]] {
                continue;
            }
            games
                .entry((agent.clone(), generation.clone()))
                .or_default()
                .push(Game {
                    opponent: index[opponent.as_str()],
                    score,
                    black,
                    seed: s.start_seed,
                    opening: s.start_mode == SlotStartMode::Opening,
                });
        }
    }
    let mut admissions = state
        .adaptive
        .as_ref()
        .unwrap()
        .generation_matchups
        .admissions
        .clone();
    let mut reports = vec![];
    let mut protection = vec![];
    for ((agent, generation), games) in games {
        if !labels.contains_key(&agent) {
            continue;
        }
        let members: Vec<_> = ranked
            .iter()
            .filter(|n| labels.get(*n) == Some(&generation))
            .collect();
        let benchmark = members[0];
        let panel: Vec<_> = members
            .iter()
            .filter(|n| ***n != agent)
            .take(3)
            .map(|n| (*n).clone())
            .collect();
        let i = index[agent.as_str()];
        let j = index[benchmark.as_str()];
        let (completed_pairs, paired_opponents) = pair_coverage(&games);
        let (probability, margin_elo, sd_elo) = estimate(p, i, j, &games);
        let coverage_met = completed_pairs >= MIN_PAIRS
            && paired_opponents >= MIN_OPPONENTS
            && panel.len() == 3
            && graph.component[i] == graph.component[j];
        let eligible = coverage_met && probability >= TEST_PROBABILITY;
        if eligible
            && !admissions
                .iter()
                .any(|r| r.agent == agent && r.generation == generation)
        {
            admissions.push(Admission {
                agent: agent.clone(),
                generation: generation.clone(),
                initial_pairs: completed_pairs,
            });
        }
        let fresh_pairs = admissions
            .iter()
            .find(|r| r.agent == agent && r.generation == generation)
            .map_or(0, |r| completed_pairs.saturating_sub(r.initial_pairs));
        let protected =
            coverage_met && fresh_pairs >= FRESH_PAIRS && probability >= PROTECT_PROBABILITY;
        if protected {
            protection.push((agent.clone(), generation.clone()));
        }
        let report = Report {
            agent,
            generation,
            benchmark: benchmark.clone(),
            panel,
            games: games.len(),
            completed_pairs,
            paired_opponents,
            probability,
            margin_elo,
            sd_elo,
            coverage_met,
            eligible,
            protected,
            fresh_pairs,
        };
        reports.push(report);
    }
    let tracking = &mut state.adaptive.as_mut().unwrap().generation_matchups;
    tracking.reports = reports;
    tracking.admissions = admissions;
    Ok(protection)
}

pub(super) fn choose(state: &TourneyState, p: &Posterior) -> Option<(usize, usize, String)> {
    let tracking = &state.adaptive.as_ref()?.generation_matchups;
    if tracking.probe_pairs >= (tracking.scheduled_pairs + 1) / PAIRS_PER_PROBE {
        return None;
    }
    let busy: BTreeSet<_> = state
        .slots
        .iter()
        .filter(|s| matches!(s.status, SlotStatus::Pending | SlotStatus::Running))
        .map(|s| {
            if s.model_a < s.model_b {
                (&s.model_a, &s.model_b)
            } else {
                (&s.model_b, &s.model_a)
            }
        })
        .collect();
    let mut choices = vec![];
    for r in tracking
        .reports
        .iter()
        .filter(|r| r.eligible && state.retired.contains(&r.agent))
    {
        let i = p.names.binary_search(&r.agent).ok()?;
        let benchmark = p.names.binary_search(&r.benchmark).ok()?;
        for opponent in &r.panel {
            if busy.contains(&if r.agent < *opponent {
                (&r.agent, opponent)
            } else {
                (opponent, &r.agent)
            }) {
                continue;
            }
            let j = p.names.binary_search(opponent).ok()?;
            let prob = model::sigmoid(p.mean[benchmark] + r.margin_elo / SCALE - p.mean[j]);
            let variance = (r.sd_elo / SCALE).powi(2);
            let information = prob * (1. - prob);
            // Approximate reduction in uncertainty about competitiveness, on
            // the same real-evidence model used for conditional eligibility.
            let utility = r.probability * (1. - r.probability) * variance.powi(2) * information
                / (1. + variance * information);
            choices.push((utility, i, j));
        }
    }
    let (_, i, j) = choices.into_iter().max_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| (b.1, b.2).cmp(&(a.1, a.2)))
    })?;
    // Ordinary exploratory pair: no persistent trial-game tag or batch IDs.
    Some((i, j, "probe_residual".into()))
}

pub(super) fn record(state: &mut TourneyState, a: &str, b: &str) {
    let conditional = state.retired.contains(a) || state.retired.contains(b);
    let tracking = &mut state.adaptive.as_mut().unwrap().generation_matchups;
    tracking.scheduled_pairs += 1;
    if conditional {
        tracking.probe_pairs += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::TourneySlot;

    fn setup() -> (TourneyState, Posterior, BTreeMap<String, f64>) {
        let mut state = super::super::tests::fixture(6);
        state.adaptive.get_or_insert_with(Default::default);
        for (i, e) in state.entrants.iter_mut().enumerate() {
            e.generation = Some(if i < 3 { "elite" } else { "older" }.into());
        }
        state.retired.insert("agent05".into());
        let names: Vec<_> = state.entrants.iter().map(|e| e.id.clone()).collect();
        let ratings = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), 1800. - i as f64 * 10.))
            .collect();
        let p = Posterior {
            names,
            mean: vec![0.; 7],
            covariance: vec![vec![0.; 7]; 7],
            factor: vec![vec![0.; 7]; 7],
            games: 0,
        };
        (state, p, ratings)
    }

    fn add_pair(state: &mut TourneyState, opponent: usize, scores: [f64; 2]) {
        let id = super::super::next_slot_id(state);
        for (leg, score) in scores.into_iter().enumerate() {
            state.slots.push(TourneySlot {
                id: id + leg,
                model_a: "agent05".into(),
                model_b: format!("agent{opponent:02}"),
                start_seed: id as u64,
                a_is_black: leg == 0,
                status: SlotStatus::Done,
                game_path: None,
                score_a: Some(score),
                round: 0,
                start_mode: SlotStartMode::Light,
            });
        }
    }

    fn report(state: &TourneyState) -> &Report {
        state
            .adaptive
            .as_ref()
            .unwrap()
            .generation_matchups
            .reports
            .iter()
            .find(|r| r.agent == "agent05" && r.generation == "elite")
            .unwrap()
    }

    #[test]
    fn missing_games_and_new_generations_do_not_qualify() {
        let (mut s, p, ratings) = setup();
        refresh(&mut s, &p, &ratings).unwrap();
        assert!(s
            .adaptive
            .as_ref()
            .unwrap()
            .generation_matchups
            .reports
            .is_empty());
        assert!(choose(&s, &p).is_none());
        // Single-colour results never become fictitious return pairs.
        for i in 0..4 {
            add_pair(&mut s, i % 3, [1., 0.5]);
        }
        for slot in &mut s.slots {
            slot.a_is_black = true;
        }
        refresh(&mut s, &p, &ratings).unwrap();
        assert_eq!(report(&s).completed_pairs, 0);
        assert!(!report(&s).eligible);
    }

    #[test]
    fn ordinary_results_confirm_without_trial_tags_or_batches() {
        let (mut s, p, ratings) = setup();
        for i in 0..4 {
            add_pair(&mut s, i % 3, [1., 0.5]);
        }
        let original = serde_json::to_value(&s.slots).unwrap();
        assert!(refresh(&mut s, &p, &ratings).unwrap().is_empty());
        assert!(report(&s).eligible);
        assert_eq!(report(&s).fresh_pairs, 0);
        assert!(!report(&s).protected);
        assert!(s.retired.contains("agent05"));
        assert_eq!(original, serde_json::to_value(&s.slots).unwrap());
        // Reloading or rechecking the same evidence does not supply confirmation.
        s = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        refresh(&mut s, &p, &ratings).unwrap();
        assert_eq!(report(&s).fresh_pairs, 0);
        // These results have no entries in scheduler.pairs at all: any ordinary
        // completed games against the generation count, regardless of scheduler.
        for i in 0..4 {
            add_pair(&mut s, i % 3, [1., 1.]);
        }
        let protection = refresh(&mut s, &p, &ratings).unwrap();
        assert_eq!(report(&s).fresh_pairs, 4);
        assert!(report(&s).protected);
        assert!(protection.contains(&("agent05".into(), "elite".into())));
    }

    #[test]
    fn conditional_pairs_share_normal_lane_and_have_a_strict_five_percent_cap() {
        let (mut s, p, ratings) = setup();
        for i in 0..4 {
            add_pair(&mut s, i % 3, [1., 0.5]);
        }
        refresh(&mut s, &p, &ratings).unwrap();
        for _ in 0..19 {
            assert!(choose(&s, &p).is_none());
            record(&mut s, "agent00", "agent01");
        }
        let (a, b, reason) = choose(&s, &p).unwrap();
        assert_eq!(p.names[a], "agent05");
        assert!(b < 3);
        assert_eq!(reason, "probe_residual");
        record(&mut s, &p.names[a], &p.names[b]);
        assert!(choose(&s, &p).is_none());
        let tracking = &s.adaptive.as_ref().unwrap().generation_matchups;
        assert_eq!((tracking.scheduled_pairs, tracking.probe_pairs), (20, 1));
    }

    #[test]
    fn coverage_and_relevance_are_required_even_for_good_results() {
        let (mut s, p, mut ratings) = setup();
        for _ in 0..5 {
            add_pair(&mut s, 0, [1., 0.5]);
        }
        refresh(&mut s, &p, &ratings).unwrap();
        assert!(!report(&s).eligible);
        add_pair(&mut s, 1, [1., 0.5]);
        add_pair(&mut s, 2, [1., 0.5]);
        refresh(&mut s, &p, &ratings).unwrap();
        assert!(report(&s).eligible);
        // A generation is relevant only if represented in the actual top five.
        for e in &mut s.entrants {
            e.generation = Some("other".into());
        }
        s.entrants[0].generation = Some("elite".into());
        ratings.insert("agent00".into(), 0.);
        refresh(&mut s, &p, &ratings).unwrap();
        assert!(!s
            .adaptive
            .as_ref()
            .unwrap()
            .generation_matchups
            .reports
            .iter()
            .any(|r| r.generation == "elite"));
    }

    #[test]
    fn old_scheduler_state_gets_no_accumulated_probe_budget() {
        let (s, _, _) = setup();
        let mut value = serde_json::to_value(s).unwrap();
        value["adaptive"]
            .as_object_mut()
            .unwrap()
            .remove("generation_matchups");
        let restored: TourneyState = serde_json::from_value(value).unwrap();
        let tracking = &restored.adaptive.unwrap().generation_matchups;
        assert_eq!(tracking.scheduled_pairs, 0);
        assert_eq!(tracking.probe_pairs, 0);
        assert!(tracking.admissions.is_empty());
    }

    #[test]
    fn normal_claim_honours_both_legs_without_reactivating_the_candidate() {
        let (mut s, p, ratings) = setup();
        for i in 0..4 {
            add_pair(&mut s, i % 3, [1., 0.5]);
        }
        for opponent in [3, 4] {
            add_pair(&mut s, opponent, [0.5, 0.5]);
        }
        refresh(&mut s, &p, &ratings).unwrap();
        let key = super::super::evidence_key(&s);
        let scheduler = s.adaptive.as_mut().unwrap();
        scheduler.cache = Some((key, std::sync::Arc::new(p)));
        scheduler.allocation_credit = [0, 1000, 0];
        scheduler.generation_matchups.scheduled_pairs = 19;
        let first = super::super::claim(&mut s).unwrap().unwrap();
        assert!(s.retired.contains("agent05"));
        let pair = s.adaptive.as_ref().unwrap().pairs.last().unwrap();
        assert_eq!(pair.agents[0], "agent05");
        assert_eq!(pair.reason, "probe_residual");
        assert_eq!(pair.slot_ids, [first, first + 1]);
        let second = super::super::claim(&mut s).unwrap().unwrap();
        assert_eq!(second, first + 1);
        assert!(s.retired.contains("agent05"));
        assert_eq!(
            s.adaptive.as_ref().unwrap().generation_matchups.probe_pairs,
            1
        );
    }
}
