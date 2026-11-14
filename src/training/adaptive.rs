//! Continuous paired sampling. Connectivity overrides every budget; unfinished
//! commitments are draws in selection only, never in ratings or lifecycle evidence.
use super::scheduling_model::{self as model, Posterior, SCALE};
use super::tournament::{
    next_slot_id, SlotStartMode, SlotStatus, TourneyFormat, TourneySlot, TourneyState,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub mod generation_matchups;

const DRAWS: usize = 8192;
const TOP_K: usize = 5;
const MIN_GAMES: usize = 40;
const MIN_OPPONENTS: usize = 6;
const WEIGHTS: [i64; 3] = [85, 10, 5];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pair {
    pub slot_ids: [usize; 2],
    pub agents: [String; 2],
    pub reason: String,
    pub evidence_games: usize,
    pub assumed_draw_slots: Vec<usize>,
    pub seed: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Exposure {
    pub games: usize,
    pub opponents: usize,
    pub top_five_probability: f64,
    pub probability_upper: f64,
    pub protected_by: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RetirementThreshold {
    /// Registered entrants eligible at the start of this evidence update.
    pub active_agents: usize,
    pub probability: f64,
    /// Missing in older checkpoints, which used a fixed 5% reactivation cutoff.
    #[serde(default)]
    pub reactivation_probability: Option<f64>,
    /// Reuse the same denominator after a restart with unchanged evidence.
    pub evidence_key: u64,
}

impl RetirementThreshold {
    pub fn from_state(state: &TourneyState) -> Self {
        let active_agents = state
            .entrants
            .iter()
            .filter(|e| !state.retired.contains(&e.id))
            .count();
        let probability = (5. / active_agents.max(1) as f64).clamp(0.0005, 0.01);
        Self {
            active_agents,
            probability,
            reactivation_probability: Some(probability),
            evidence_key: evidence_key(state),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Confirmation {
    pub leader: String,
    pub opponent: String,
    pub pair_slots: Vec<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scheduler {
    pub version: u32,
    pub generation_order: Vec<String>,
    pub pairs: Vec<Pair>,
    pub allocation_credit: [i64; 3],
    pub lane_pairs: [usize; 3],
    pub learning_picks: usize,
    pub anchor_picks: usize,
    pub confirmations: Vec<Confirmation>,
    pub exposure: BTreeMap<String, Exposure>,
    #[serde(default)]
    pub retirement_threshold: Option<RetirementThreshold>,
    #[serde(default)]
    pub generation_matchups: generation_matchups::State,
    pub completed_games: usize,
    pub actual_components: usize,
    pub assumed_components: usize,
    pub archived_knockouts: Vec<usize>,
    #[serde(default)]
    pub archived_slots: Vec<usize>,
    pub error: Option<String>,
    #[serde(skip)]
    cache: Option<(u64, Arc<Posterior>)>,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            version: 1,
            generation_order: ["handcrafted", "v2_v3", "v4_v5", "search", "v6", "v6_search"]
                .map(String::from)
                .to_vec(),
            pairs: vec![],
            allocation_credit: [0; 3],
            lane_pairs: [0; 3],
            learning_picks: 0,
            anchor_picks: 0,
            confirmations: vec![],
            exposure: BTreeMap::new(),
            retirement_threshold: None,
            generation_matchups: generation_matchups::State::default(),
            completed_games: 0,
            actual_components: 0,
            assumed_components: 0,
            archived_knockouts: vec![],
            archived_slots: vec![],
            error: None,
            cache: None,
        }
    }
}

pub fn migrate(state: &mut TourneyState) -> Result<(), String> {
    if state.slots.iter().any(|s| s.status == SlotStatus::Running) {
        return Err("adaptive migration requires running games to finish first".into());
    }
    if state.format == TourneyFormat::Adaptive {
        return Ok(());
    }
    if state.format != TourneyFormat::Knockout {
        return Err("adaptive migration requires a knockout run".into());
    }
    let archived: Vec<_> = state
        .knockouts
        .iter()
        .filter(|t| !t.complete)
        .map(|t| t.id)
        .collect();
    let scheduler = state.adaptive.get_or_insert_with(Default::default);
    for id in archived {
        if !scheduler.archived_knockouts.contains(&id) {
            scheduler.archived_knockouts.push(id);
        }
    }
    scheduler.cache = None;
    scheduler.error = None;
    if state
        .runtime_control
        .pause_reason
        .starts_with("adaptive scheduling failed:")
    {
        state.runtime_control.assignments_paused = false;
        state.runtime_control.pause_reason.clear();
    }
    state.format = TourneyFormat::Adaptive;
    // Finish the return leg of every started pair, but do not carry a whole
    // unstarted bracket round into the replacement sampler. Preserve cancelled
    // assignments explicitly rather than deleting history or retrying them.
    let pair_key = |s: &TourneySlot| {
        let (a, b) = if s.model_a < s.model_b {
            (&s.model_a, &s.model_b)
        } else {
            (&s.model_b, &s.model_a)
        };
        (
            a.clone(),
            b.clone(),
            s.start_seed,
            s.start_mode == SlotStartMode::Opening,
        )
    };
    let started: BTreeSet<_> = state
        .slots
        .iter()
        .filter(|s| matches!(s.status, SlotStatus::Done | SlotStatus::Aborted))
        .map(pair_key)
        .collect();
    for slot in &mut state.slots {
        if matches!(slot.status, SlotStatus::Pending | SlotStatus::Aborted)
            && !started.contains(&pair_key(slot))
        {
            slot.status = SlotStatus::Cancelled;
            state
                .adaptive
                .as_mut()
                .unwrap()
                .archived_slots
                .push(slot.id);
        }
    }
    state.retired.clear();
    refresh(state)
}

pub fn rollback(state: &mut TourneyState) -> Result<(), String> {
    if state.format == TourneyFormat::Knockout && state.adaptive.is_some() {
        return Ok(());
    }
    if state.slots.iter().any(|s| s.status == SlotStatus::Running) {
        return Err("scheduler rollback requires running games to finish".into());
    }
    if state.adaptive.is_none() {
        return Err("this run has no adaptive scheduler to roll back".into());
    }
    state.format = TourneyFormat::Knockout;
    state.adaptive.as_mut().unwrap().error = None;
    if state
        .runtime_control
        .pause_reason
        .starts_with("adaptive scheduling failed:")
    {
        state.runtime_control.assignments_paused = false;
        state.runtime_control.pause_reason.clear();
    }
    Ok(())
}

fn evidence_key(state: &TourneyState) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for e in &state.entrants {
        e.id.hash(&mut h);
        e.generation.hash(&mut h);
    }
    for s in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
        s.id.hash(&mut h);
        s.model_a.hash(&mut h);
        s.model_b.hash(&mut h);
        s.score_a.map(f64::to_bits).hash(&mut h);
        s.a_is_black.hash(&mut h);
        s.start_seed.hash(&mut h);
        (s.start_mode == SlotStartMode::Opening).hash(&mut h);
    }
    if let Some(a) = &state.adaptive {
        a.generation_order.hash(&mut h);
    }
    h.finish()
}

pub struct Graph {
    pub reach: Vec<Vec<bool>>,
    pub component: Vec<usize>,
    pub groups: Vec<Vec<usize>>,
}
impl Graph {
    pub fn new(state: &TourneyState, names: &[String], draws: bool) -> Self {
        let n = names.len();
        let index: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, s)| (s.as_str(), i))
            .collect();
        let mut reach = vec![vec![false; n]; n];
        for (i, row) in reach.iter_mut().enumerate() {
            row[i] = true;
        }
        for s in &state.slots {
            let score = match s.status {
                SlotStatus::Done => s.score_a,
                SlotStatus::Pending | SlotStatus::Running if draws => Some(0.5),
                _ => None,
            };
            if let (Some(score), Some(&a), Some(&b)) = (
                score,
                index.get(s.model_a.as_str()),
                index.get(s.model_b.as_str()),
            ) {
                if score > 0. {
                    reach[a][b] = true;
                }
                if score < 1. {
                    reach[b][a] = true;
                }
            }
        }
        for k in 0..n {
            let row = reach[k].clone();
            for ri in &mut reach {
                if ri[k] {
                    for (v, w) in ri.iter_mut().zip(&row) {
                        *v |= w;
                    }
                }
            }
        }
        let mut component = vec![usize::MAX; n];
        let mut groups = vec![];
        for i in 0..n {
            if component[i] != usize::MAX {
                continue;
            }
            let mut group = vec![];
            for j in i..n {
                if reach[i][j] && reach[j][i] {
                    component[j] = groups.len();
                    group.push(j);
                }
            }
            groups.push(group);
        }
        Self {
            reach,
            component,
            groups,
        }
    }
}

fn registered(state: &TourneyState, p: &Posterior) -> Vec<usize> {
    let ids: BTreeSet<_> = state.entrants.iter().map(|e| &e.id).collect();
    p.names
        .iter()
        .enumerate()
        .filter_map(|(i, s)| ids.contains(s).then_some(i))
        .collect()
}

fn exposure(state: &TourneyState, p: &Posterior) -> Vec<BTreeMap<usize, usize>> {
    let index: BTreeMap<_, _> = p
        .names
        .iter()
        .enumerate()
        .map(|(i, s)| (s.as_str(), i))
        .collect();
    let mut out = vec![BTreeMap::new(); p.names.len()];
    for s in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
        let (a, b) = (index[s.model_a.as_str()], index[s.model_b.as_str()]);
        *out[a].entry(b).or_default() += 1;
        *out[b].entry(a).or_default() += 1;
    }
    out
}

fn generation_champions(
    state: &TourneyState,
    ratings: &BTreeMap<String, f64>,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::<String, String>::new();
    for e in &state.entrants {
        let Some(label) = super::generations::label(e).ok().flatten() else {
            continue;
        };
        let candidate = out.entry(label.to_string()).or_insert_with(|| e.id.clone());
        if ratings[&e.id] > ratings[candidate]
            || (ratings[&e.id] == ratings[candidate] && e.id < *candidate)
        {
            *candidate = e.id.clone();
        }
    }
    out
}

// Minimum-bottleneck monotone path, at most three edges / two bridge agents.
fn bridge_path(p: &Posterior, registered: &[usize], a: usize, b: usize) -> Vec<usize> {
    let (low, high) = if p.mean[a] <= p.mean[b] {
        (a, b)
    } else {
        (b, a)
    };
    if p.mean[high] - p.mean[low] <= 9_f64.ln() {
        return vec![a, b];
    }
    let mut middle: Vec<_> = registered
        .iter()
        .copied()
        .filter(|&i| i != low && i != high && p.mean[i] > p.mean[low] && p.mean[i] < p.mean[high])
        .collect();
    middle.sort_by(|&i, &j| p.mean[i].total_cmp(&p.mean[j]).then(i.cmp(&j)));
    let mut nodes = vec![low];
    nodes.extend(middle);
    nodes.push(high);
    let n = nodes.len();
    let mut cost = vec![vec![f64::INFINITY; n]; 4];
    let mut prev = vec![vec![0; n]; 4];
    cost[0][0] = 0.;
    for steps in 1..=3 {
        for j in 1..n {
            for i in 0..j {
                let next = cost[steps - 1][i].max(p.mean[nodes[j]] - p.mean[nodes[i]]);
                if next < cost[steps][j] {
                    cost[steps][j] = next;
                    prev[steps][j] = i;
                }
            }
        }
    }
    let mut steps = (1..=3)
        .min_by(|&i, &j| cost[i][n - 1].total_cmp(&cost[j][n - 1]))
        .unwrap();
    let mut j = n - 1;
    let mut path = vec![nodes[j]];
    while steps > 0 {
        j = prev[steps][j];
        path.push(nodes[j]);
        steps -= 1;
    }
    path.reverse();
    path
}

fn anchors(
    state: &TourneyState,
    p: &Posterior,
    ratings: &BTreeMap<String, f64>,
) -> Vec<Vec<usize>> {
    let champions = generation_champions(state, ratings);
    let ids: Vec<_> = state
        .adaptive
        .as_ref()
        .unwrap()
        .generation_order
        .iter()
        .filter_map(|g| champions.get(g))
        .map(|s| p.names.binary_search(s).unwrap())
        .collect();
    let registered = registered(state, p);
    ids.windows(2)
        .map(|w| bridge_path(p, &registered, w[0], w[1]))
        .collect()
}

fn leader(state: &TourneyState, p: &Posterior, ratings: &BTreeMap<String, f64>) -> Option<usize> {
    let graph = Graph::new(state, &p.names, false);
    let group = graph.groups.iter().max_by_key(|g| g.len())?;
    let ids: BTreeSet<_> = registered(state, p).into_iter().collect();
    group
        .iter()
        .copied()
        .filter(|i| ids.contains(i) && !state.retired.contains(&p.names[*i]))
        .max_by(|&i, &j| {
            ratings[&p.names[i]]
                .total_cmp(&ratings[&p.names[j]])
                .then(j.cmp(&i))
        })
}

fn normal_cdf(x: f64) -> f64 {
    let t = 1. / (1. + 0.2316419 * x.abs());
    let density = (-0.5 * x * x).exp() / (2. * std::f64::consts::PI).sqrt();
    let tail = density
        * t
        * (0.319381530
            + t * (-0.356563782 + t * (1.781477937 + t * (-1.821255978 + t * 1.330274429))));
    if x >= 0. {
        1. - tail
    } else {
        tail
    }
}

fn residual(
    state: &TourneyState,
    p: &Posterior,
    champion: usize,
    opponent: usize,
    only: Option<&BTreeSet<usize>>,
) -> (f64, f64, usize) {
    let mut games = vec![];
    for s in &state.slots {
        if s.status != SlotStatus::Done || only.is_some_and(|ids| !ids.contains(&s.id)) {
            continue;
        }
        let (score, black) = if s.model_a == p.names[opponent] && s.model_b == p.names[champion] {
            (s.score_a.unwrap(), s.a_is_black)
        } else if s.model_b == p.names[opponent] && s.model_a == p.names[champion] {
            (1. - s.score_a.unwrap(), !s.a_is_black)
        } else {
            continue;
        };
        games.push((
            p.mean[opponent] - p.mean[champion]
                + if black {
                    p.mean[p.names.len()]
                } else {
                    -p.mean[p.names.len()]
                },
            score,
        ));
    }
    let precision = (SCALE / 100.).powi(2);
    let mut mu = 0.;
    let mut h = precision;
    for _ in 0..40 {
        let mut g = mu * precision;
        h = precision;
        for &(offset, score) in &games {
            let prob = model::sigmoid(offset + mu);
            g += prob - score;
            h += prob * (1. - prob);
        }
        let step = g / h;
        mu -= step;
        if step.abs() < 1e-9 {
            break;
        }
    }
    let v = (p.covariance[champion][champion] + p.covariance[opponent][opponent]
        - 2. * p.covariance[champion][opponent])
        .max(0.);
    (mu, (1. / h + v).sqrt(), games.len())
}

fn wilson_upper(success: usize, total: usize) -> f64 {
    let n = total as f64;
    let z = 1.959963985;
    let p = success as f64 / n;
    (p + z * z / (2. * n) + z * (p * (1. - p) / n + z * z / (4. * n * n)).sqrt()) / (1. + z * z / n)
}

fn inactive_under_policy(was_inactive: bool, e: &Exposure, in_main: bool, cutoff: f64) -> bool {
    if !e.protected_by.is_empty()
        || e.top_five_probability > cutoff
        || e.games < MIN_GAMES
        || e.opponents < MIN_OPPONENTS
        || !in_main
    {
        return false;
    }
    was_inactive || e.probability_upper < cutoff
}

pub fn refresh(state: &mut TourneyState) -> Result<(), String> {
    if state.format != TourneyFormat::Adaptive {
        return Ok(());
    }
    state.adaptive.get_or_insert_with(Default::default);
    if state.adaptive.as_ref().unwrap().version != 1 {
        return Err("unsupported adaptive scheduler version".into());
    }
    let key = evidence_key(state);
    if state
        .adaptive
        .as_ref()
        .unwrap()
        .cache
        .as_ref()
        .is_some_and(|(k, _)| *k == key)
    {
        return Ok(());
    }
    let p = Arc::new(model::fit(state, false)?);
    let published = super::order_neutral::from_state(state);
    let registered = registered(state, &p);
    let mut counts = vec![0; p.names.len()];
    for draw in p.draws(DRAWS, state.seed_base ^ key) {
        let mut rank = registered.clone();
        rank.sort_unstable_by(|&i, &j| draw[j].total_cmp(&draw[i]).then(i.cmp(&j)));
        for &i in rank.iter().take(TOP_K) {
            counts[i] += 1;
        }
    }
    let coverage = exposure(state, &p);
    let graph = Graph::new(state, &p.names, false);
    let main = graph
        .groups
        .iter()
        .max_by_key(|g| g.len())
        .cloned()
        .unwrap_or_default();
    let mut protected = BTreeMap::<String, Vec<String>>::new();
    for (name, generation) in generation_matchups::refresh(state, &p, &published.ratings)? {
        protected
            .entry(name)
            .or_default()
            .push(format!("generation_matchup:{generation}"));
    }
    for (generation, name) in generation_champions(state, &published.ratings) {
        protected
            .entry(name)
            .or_default()
            .push(format!("generation:{generation}"));
    }
    for path in anchors(state, &p, &published.ratings) {
        for &i in path.iter().skip(1).take(path.len().saturating_sub(2)) {
            protected
                .entry(p.names[i].clone())
                .or_default()
                .push("generation_bridge".into());
        }
    }
    if let Some(champion) = leader(state, &p, &published.ratings) {
        for confirmation in &state.adaptive.as_ref().unwrap().confirmations {
            if confirmation.leader != p.names[champion] || confirmation.pair_slots.len() < 4 {
                continue;
            }
            let ids: BTreeSet<_> = confirmation.pair_slots.iter().flatten().copied().collect();
            let opponent = p
                .names
                .binary_search(&confirmation.opponent)
                .map_err(|_| "unknown counter opponent")?;
            let (mu, sd, n) = residual(state, &p, champion, opponent, Some(&ids));
            if n == 8 && normal_cdf(mu / sd) >= 0.95 {
                protected
                    .entry(p.names[opponent].clone())
                    .or_default()
                    .push(format!("champion_counter:{}", p.names[champion]));
            }
        }
    }
    // Freeze the input pool for this whole pass. Lifecycle changes must not make
    // later entrants face a different cutoff, or trigger a fixed-point cascade.
    let mut threshold = state
        .adaptive
        .as_ref()
        .unwrap()
        .retirement_threshold
        .filter(|t| t.evidence_key == key)
        .unwrap_or_else(|| RetirementThreshold::from_state(state));
    threshold.reactivation_probability = Some(threshold.probability);
    // Old checkpoints used a fixed 1% cutoff. Recheck those decisions once;
    // subsequent updates retain inactivity only within the Monte Carlo band.
    let recheck_legacy = state
        .adaptive
        .as_ref()
        .unwrap()
        .retirement_threshold
        .is_none();
    let mut report = BTreeMap::new();
    let mut inactive = BTreeSet::new();
    for i in registered {
        let games = coverage[i].values().sum();
        let opponents = coverage[i]
            .iter()
            .filter(|(j, n)| **n >= 2 && main.contains(j))
            .count();
        let probability = counts[i] as f64 / DRAWS as f64;
        let upper = wilson_upper(counts[i], DRAWS);
        let reasons = protected.remove(&p.names[i]).unwrap_or_default();
        let e = Exposure {
            games,
            opponents,
            top_five_probability: probability,
            probability_upper: upper,
            protected_by: reasons,
        };
        if inactive_under_policy(
            !recheck_legacy && state.retired.contains(&p.names[i]),
            &e,
            main.contains(&i),
            threshold.probability,
        ) {
            inactive.insert(p.names[i].clone());
        }
        report.insert(p.names[i].clone(), e);
    }
    state.retired = inactive;
    let assumed_components = Graph::new(state, &p.names, true).groups.len();
    let a = state.adaptive.as_mut().unwrap();
    a.completed_games = p.games;
    a.actual_components = graph.groups.len();
    a.assumed_components = assumed_components;
    a.exposure = report;
    a.retirement_threshold = Some(threshold);
    a.error = None;
    a.cache = Some((key, p));
    Ok(())
}

fn available_pairs(state: &TourneyState, p: &Posterior, active: &[usize]) -> Vec<(usize, usize)> {
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
    let all: Vec<_> = active
        .iter()
        .enumerate()
        .flat_map(|(k, &i)| active[k + 1..].iter().map(move |&j| (i, j)))
        .collect();
    let free: Vec<_> = all
        .iter()
        .copied()
        .filter(|&(i, j)| !busy.contains(&(&p.names[i], &p.names[j])))
        .collect();
    if free.is_empty() {
        all
    } else {
        free
    }
}

fn choose(
    state: &mut TourneyState,
    p: &Posterior,
    seed: u64,
) -> Result<(usize, usize, String), String> {
    let actual = state
        .adaptive
        .as_ref()
        .unwrap()
        .cache
        .as_ref()
        .unwrap()
        .1
        .clone();
    let graph = Graph::new(state, &p.names, true);
    state.adaptive.as_mut().unwrap().assumed_components = graph.groups.len();
    let active: Vec<_> = registered(state, p)
        .into_iter()
        .filter(|&i| !state.retired.contains(&p.names[i]))
        .collect();
    if active.len() < 2 {
        return Err("adaptive scheduler needs at least two eligible agents".into());
    }
    let pairs = available_pairs(state, p, &active);
    let draws = p.draws(64, seed);
    let information = |i, j| p.information_score(i, j, &draws);
    let cross: Vec<_> = pairs
        .iter()
        .copied()
        .filter(|&(i, j)| graph.component[i] != graph.component[j])
        .collect();
    if !cross.is_empty() {
        let utility = |i: usize, j: usize| {
            let merged: BTreeSet<_> = (0..p.names.len())
                .filter(|&k| {
                    (graph.reach[i][k] && graph.reach[k][j])
                        || (graph.reach[j][k] && graph.reach[k][i])
                })
                .map(|k| graph.component[k])
                .chain([graph.component[i], graph.component[j]])
                .collect();
            (merged.len() - 1) as f64 * p.outcome_information(i, j, &draws)
        };
        let &(i, j) = cross
            .iter()
            .max_by(|&&(a, b), &&(c, d)| {
                utility(a, b)
                    .total_cmp(&utility(c, d))
                    .then_with(|| information(a, b).total_cmp(&information(c, d)))
                    .then_with(|| (c, d).cmp(&(a, b)))
            })
            .unwrap();
        return Ok((i, j, "connectivity".into()));
    }
    let selection_state = draw_completed(state);
    let ratings = super::order_neutral::from_state(&selection_state).ratings;
    let paths = anchors(state, p, &ratings);
    let anchor_pairs: Vec<_> = paths
        .iter()
        .flat_map(|v| v.windows(2).map(|w| (w[0], w[1])))
        .filter(|&(i, j)| active.contains(&i) && active.contains(&j))
        .filter(|&(i, j)| pairs.contains(&(i.min(j), i.max(j))))
        .collect();
    let a = state.adaptive.as_mut().unwrap();
    for (credit, weight) in a.allocation_credit.iter_mut().zip(WEIGHTS) {
        *credit += weight;
    }
    let lane = (0..3)
        .max_by_key(|&i| (a.allocation_credit[i], std::cmp::Reverse(i)))
        .unwrap();
    a.allocation_credit[lane] -= 100;
    let lane = if lane == 2 && anchor_pairs.is_empty() {
        0
    } else {
        lane
    };
    a.lane_pairs[lane] += 1;
    if lane == 2 {
        let pair = anchor_pairs[a.anchor_picks % anchor_pairs.len()];
        a.anchor_picks += 1;
        return Ok((pair.0, pair.1, "generation_anchor".into()));
    }
    if lane == 1 {
        if let Some(pair) = generation_matchups::choose(state, &actual) {
            return Ok(pair);
        }
        // Selection, including the champion identity, uses the draw-completed
        // scheduling state. Published leader identity is used for counter protection.
        let champion =
            leader(&selection_state, p, &ratings).ok_or("no eligible scheduling champion")?;
        let candidates: Vec<_> = active
            .iter()
            .copied()
            .filter(|&i| {
                i != champion
                    && pairs
                        .iter()
                        .any(|&(a, b)| (a == i && b == champion) || (b == i && a == champion))
            })
            .collect();
        let candidates = if candidates.is_empty() {
            active.iter().copied().filter(|&i| i != champion).collect()
        } else {
            candidates
        };
        let coverage = exposure(&selection_state, p);
        if state.adaptive.as_ref().unwrap().lane_pairs[1] % 2 == 1 {
            if let Some(&opponent) = candidates
                .iter()
                .find(|&&i| !coverage[champion].contains_key(&i))
            {
                return Ok((champion, opponent, "probe_discovery".into()));
            }
        }
        let continuing = state
            .adaptive
            .as_ref()
            .unwrap()
            .confirmations
            .iter()
            .find(|c| {
                c.leader == p.names[champion]
                    && c.pair_slots.len() < 4
                    && candidates.iter().any(|&i| p.names[i] == c.opponent)
            });
        if let Some(c) = continuing {
            return Ok((
                champion,
                p.names.binary_search(&c.opponent).unwrap(),
                "probe_confirmation".into(),
            ));
        }
        let opponent = *candidates
            .iter()
            .max_by(|&&i, &&j| {
                let score = |k| {
                    let (mu, sd, _) = residual(&selection_state, p, champion, k, None);
                    mu + sd * (draws[0][k] - p.mean[k]) / p.covariance[k][k].sqrt()
                };
                score(i).total_cmp(&score(j)).then(j.cmp(&i))
            })
            .unwrap();
        let (mu, sd, n) = residual(state, &actual, champion, opponent, None);
        if n >= 4
            && normal_cdf(mu / sd) > 0.9
            && !state
                .adaptive
                .as_ref()
                .unwrap()
                .confirmations
                .iter()
                .any(|c| c.leader == p.names[champion] && c.opponent == p.names[opponent])
        {
            state
                .adaptive
                .as_mut()
                .unwrap()
                .confirmations
                .push(Confirmation {
                    leader: p.names[champion].clone(),
                    opponent: p.names[opponent].clone(),
                    pair_slots: vec![],
                });
            return Ok((champion, opponent, "probe_confirmation".into()));
        }
        return Ok((champion, opponent, "probe_residual".into()));
    }
    let a = state.adaptive.as_mut().unwrap();
    a.learning_picks += 1;
    if a.learning_picks % 2 == 1 {
        let counts = exposure(&selection_state, p);
        let under = active.iter().copied().filter(|&i| {
            counts[i].values().sum::<usize>() < MIN_GAMES
                || counts[i].values().filter(|&&n| n >= 2).count() < MIN_OPPONENTS
        });
        if let Some(i) = under.min_by_key(|&i| (counts[i].values().sum::<usize>(), i)) {
            let opponents: Vec<_> = pairs
                .iter()
                .filter_map(|&(a, b)| {
                    if a == i {
                        Some(b)
                    } else if b == i {
                        Some(a)
                    } else {
                        None
                    }
                })
                .collect();
            if let Some(&j) = opponents.iter().max_by(|&&a, &&b| {
                let novel = |j| counts[i].get(&j).copied().unwrap_or(0) < 2;
                novel(a)
                    .cmp(&novel(b))
                    .then_with(|| information(i, a).total_cmp(&information(i, b)))
                    .then(b.cmp(&a))
            }) {
                return Ok((i, j, "placement".into()));
            }
        }
    }
    // Reduce uncertainty at the membership boundary. Pure contrast variance
    // spends too much of the learning budget sorting certain winners or losers.
    // The Thompson gate still explores plausible top-five entrants, while this
    // weight favors agents whose inclusion in the top five is unresolved.
    let mut membership = vec![0.; p.names.len()];
    for draw in &draws {
        let mut rank = active.clone();
        rank.sort_unstable_by(|&i, &j| draw[j].total_cmp(&draw[i]).then(i.cmp(&j)));
        for i in rank.into_iter().take(TOP_K) {
            membership[i] += 1. / draws.len() as f64;
        }
    }
    let utility = |i, j| {
        let uncertain = |k: usize| membership[k] * (1. - membership[k]);
        information(i, j) * (uncertain(i) + uncertain(j) + 1. / draws.len() as f64)
    };
    let mut ranked = active;
    ranked.sort_unstable_by(|&i, &j| draws[0][j].total_cmp(&draws[0][i]).then(i.cmp(&j)));
    let top: BTreeSet<_> = ranked.into_iter().take(TOP_K).collect();
    let &(i, j) = pairs
        .iter()
        .filter(|&&(i, j)| top.contains(&i) || top.contains(&j))
        .max_by(|&&(a, b), &&(c, d)| {
            utility(a, b)
                .total_cmp(&utility(c, d))
                .then_with(|| (c, d).cmp(&(a, b)))
        })
        .ok_or("no informative eligible pair")?;
    Ok((i, j, "top_five_information".into()))
}

pub fn draw_completed(state: &TourneyState) -> TourneyState {
    let mut copy = state.clone();
    for slot in &mut copy.slots {
        if matches!(slot.status, SlotStatus::Pending | SlotStatus::Running) {
            slot.status = SlotStatus::Done;
            slot.score_a = Some(0.5);
        }
    }
    copy
}

pub fn start_mode(state: &TourneyState, a: &str, b: &str) -> SlotStartMode {
    if state.slots.iter().any(|s| {
        matches!(
            s.status,
            SlotStatus::Done | SlotStatus::Pending | SlotStatus::Running
        ) && ((s.model_a == a && s.model_b == b) || (s.model_a == b && s.model_b == a))
    }) {
        SlotStartMode::Light
    } else {
        SlotStartMode::Opening
    }
}

pub fn claim(state: &mut TourneyState) -> Result<Option<usize>, String> {
    refresh(state)?;
    // The oldest committed leg wins regardless of lane, worker, or retirement.
    if let Some(s) = state
        .slots
        .iter_mut()
        .find(|s| s.status == SlotStatus::Pending)
    {
        s.status = SlotStatus::Running;
        return Ok(Some(s.id));
    }
    let virtual_fit = model::fit(state, true)?;
    let id = next_slot_id(state);
    let seed = state
        .seed_base
        .wrapping_add((id as u64).wrapping_mul(0x9e3779b97f4a7c15));
    let (i, j, reason) = choose(state, &virtual_fit, seed)?;
    let (a, b) = (virtual_fit.names[i].clone(), virtual_fit.names[j].clone());
    let mode = start_mode(state, &a, &b);
    let assumed = state
        .slots
        .iter()
        .filter(|s| matches!(s.status, SlotStatus::Pending | SlotStatus::Running))
        .map(|s| s.id)
        .collect();
    let first_black = super::tournament::assign_a_is_black(state, &a, &b, id);
    for offset in 0..2 {
        state.slots.push(TourneySlot {
            id: id + offset,
            model_a: a.clone(),
            model_b: b.clone(),
            start_seed: seed,
            a_is_black: if offset == 0 {
                first_black
            } else {
                !first_black
            },
            status: if offset == 0 {
                SlotStatus::Running
            } else {
                SlotStatus::Pending
            },
            game_path: None,
            score_a: None,
            round: 0,
            start_mode: mode,
        });
    }
    let assumed_components = Graph::new(state, &virtual_fit.names, true).groups.len();
    generation_matchups::record(state, &a, &b);
    let scheduler = state.adaptive.as_mut().unwrap();
    scheduler.assumed_components = assumed_components;
    if reason == "probe_confirmation" {
        if let Some(c) = scheduler
            .confirmations
            .iter_mut()
            .find(|c| c.leader == a && c.opponent == b && c.pair_slots.len() < 4)
        {
            c.pair_slots.push([id, id + 1]);
        }
    }
    scheduler.pairs.push(Pair {
        slot_ids: [id, id + 1],
        agents: [a, b],
        reason,
        evidence_games: scheduler.completed_games,
        assumed_draw_slots: assumed,
        seed,
    });
    Ok(Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{
        build_schedule, claim_for_worker, TourneyConfig, TourneyEntrant,
    };

    pub(super) fn fixture(n: usize) -> TourneyState {
        let mut s = build_schedule(&TourneyConfig {
            format: TourneyFormat::Adaptive,
            entrants: (0..n)
                .map(|i| TourneyEntrant {
                    id: format!("agent{i:02}"),
                    model: "unused".into(),
                    engine: None,
                    generation: None,
                })
                .collect(),
            ..Default::default()
        });
        s.order_neutral_only = true;
        s
    }

    fn result(s: &mut TourneyState, a: usize, b: usize, score: f64, seed: u64) {
        s.slots.push(TourneySlot {
            id: next_slot_id(s),
            model_a: format!("agent{a:02}"),
            model_b: format!("agent{b:02}"),
            start_seed: seed,
            a_is_black: seed.is_multiple_of(2),
            status: SlotStatus::Done,
            game_path: None,
            score_a: Some(score),
            round: 0,
            start_mode: SlotStartMode::Light,
        });
    }

    #[test]
    fn every_worker_repairs_connectivity_without_repeating_reserved_connections() {
        let mut s = fixture(8);
        let cfg = TourneyConfig {
            jobs: 8,
            ..Default::default()
        };
        for worker in 0..8 {
            assert!(claim_for_worker(&mut s, &cfg, worker, true).is_some());
        }
        let a = s.adaptive.as_ref().unwrap();
        assert_eq!(a.pairs.len(), 4);
        assert!(a.pairs.iter().all(|p| p.reason == "connectivity"));
        assert_eq!(a.lane_pairs, [0; 3]);
        let names: Vec<_> = s.entrants.iter().map(|e| e.id.clone()).collect();
        assert_eq!(Graph::new(&s, &names, false).groups.len(), 8);
        assert_eq!(Graph::new(&s, &names, true).groups.len(), 4);
        assert!(s.slots.iter().all(|g| g.score_a.is_none()));
        assert_eq!(s.adaptive.as_ref().unwrap().completed_games, 0);
    }

    #[test]
    fn decisive_completion_restores_connectivity_priority() {
        let mut s = fixture(2);
        claim(&mut s).unwrap();
        claim(&mut s).unwrap();
        assert_eq!(s.adaptive.as_ref().unwrap().pairs[0].reason, "connectivity");
        let names: Vec<_> = s.entrants.iter().map(|e| e.id.clone()).collect();
        assert_eq!(Graph::new(&s, &names, true).groups.len(), 1);
        for slot in &mut s.slots {
            slot.status = SlotStatus::Done;
            slot.score_a = Some(1.);
        }
        claim(&mut s).unwrap();
        assert_eq!(s.adaptive.as_ref().unwrap().pairs[1].reason, "connectivity");
        assert_eq!(s.slots[2].start_mode, SlotStartMode::Light);
    }

    #[test]
    fn fantasy_fit_equals_an_explicit_draw_completion_without_mutation() {
        let mut s = fixture(3);
        result(&mut s, 0, 1, 1., 0);
        claim(&mut s).unwrap();
        let before = serde_json::to_value(&s.slots).unwrap();
        let fantasy = model::fit(&s, true).unwrap();
        let explicit = model::fit(&draw_completed(&s), false).unwrap();
        assert_eq!(fantasy.mean, explicit.mean);
        assert_eq!(fantasy.covariance, explicit.covariance);
        assert_eq!(serde_json::to_value(&s.slots).unwrap(), before);
        assert_eq!(model::fit(&s, false).unwrap().games, 1);
    }

    #[test]
    fn pending_return_leg_survives_restart_and_retirement() {
        let mut s = fixture(3);
        let first = claim(&mut s).unwrap().unwrap();
        s.slots[first].status = SlotStatus::Done;
        s.slots[first].score_a = Some(0.);
        let mut restored: TourneyState =
            serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        restored.retired.insert(s.slots[first].model_a.clone());
        assert_eq!(claim(&mut restored).unwrap(), Some(first + 1));
        assert_eq!(restored.adaptive.as_ref().unwrap().pairs.len(), 1);
        assert_eq!(
            restored.slots[first + 1].start_seed,
            s.slots[first].start_seed
        );
        assert_ne!(
            restored.slots[first + 1].a_is_black,
            s.slots[first].a_is_black
        );
    }

    #[test]
    fn migration_keeps_slots_and_refuses_running_games() {
        let mut s = fixture(3);
        s.format = TourneyFormat::Knockout;
        result(&mut s, 0, 1, 0.5, 0);
        let done = serde_json::to_value(&s.slots).unwrap();
        migrate(&mut s).unwrap();
        assert_eq!(serde_json::to_value(&s.slots).unwrap(), done);
        assert_eq!(s.format, TourneyFormat::Adaptive);
        s.slots[0].status = SlotStatus::Running;
        assert!(migrate(&mut s).unwrap_err().contains("finish first"));
    }

    #[test]
    fn symmetric_exposure_does_not_inactivate_and_restart_is_reproducible() {
        let mut s = fixture(7);
        for a in 0..7 {
            for b in a + 1..7 {
                for k in 0..8 {
                    result(&mut s, a, b, 0.5, k);
                }
            }
        }
        refresh(&mut s).unwrap();
        assert!(s.retired.is_empty());
        assert!(s
            .adaptive
            .as_ref()
            .unwrap()
            .exposure
            .values()
            .all(|e| e.games == 48 && e.opponents == 6 && e.top_five_probability > 0.6));
        let mut restored: TourneyState =
            serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
        claim(&mut s).unwrap();
        claim(&mut restored).unwrap();
        assert_eq!(
            serde_json::to_value(&s.adaptive.as_ref().unwrap().pairs).unwrap(),
            serde_json::to_value(&restored.adaptive.as_ref().unwrap().pairs).unwrap()
        );
    }

    #[test]
    fn allocations_are_exact_over_one_hundred_pairs_when_connected() {
        let mut s = fixture(4);
        s.entrants[0].generation = Some("handcrafted".into());
        s.entrants[1].generation = Some("v6".into());
        for a in 0..4 {
            for b in a + 1..4 {
                result(&mut s, a, b, 0.5, 0);
            }
        }
        refresh(&mut s).unwrap();
        let p = model::fit(&s, true).unwrap();
        for seed in 0..100 {
            let (_, _, reason) = choose(&mut s, &p, seed).unwrap();
            assert_ne!(reason, "connectivity");
        }
        assert_eq!(s.adaptive.as_ref().unwrap().lane_pairs, [85, 10, 5]);
    }

    #[test]
    fn weak_exposed_agent_retires_but_separated_or_generation_champion_does_not() {
        let mut s = fixture(8);
        for a in 0..7 {
            for b in a + 1..7 {
                for k in 0..8 {
                    result(&mut s, a, b, 0.5, k);
                }
            }
        }
        for a in 0..7 {
            for k in 0..30 {
                result(&mut s, a, 7, if k == 0 { 0.5 } else { 1. }, k);
            }
        }
        refresh(&mut s).unwrap();
        assert!(s.retired.contains("agent07"));
        s.entrants[7].generation = Some("handcrafted".into());
        refresh(&mut s).unwrap();
        assert!(!s.retired.contains("agent07"));
        assert!(!s.adaptive.as_ref().unwrap().exposure["agent07"]
            .protected_by
            .is_empty());
        s.entrants[7].generation = None;
        for g in &mut s.slots {
            if g.model_b == "agent07" {
                g.score_a = Some(1.);
            }
        }
        refresh(&mut s).unwrap();
        assert!(!s.retired.contains("agent07"));
    }

    #[test]
    fn normal_probabilities_and_monte_carlo_guard_are_conservative() {
        assert!((normal_cdf(0.) - 0.5).abs() < 1e-6);
        assert!(normal_cdf(1.96) > 0.974 && normal_cdf(1.96) < 0.976);
        assert!(wilson_upper(82, DRAWS) > 0.01);
        assert!(wilson_upper(0, DRAWS) < 0.01);
    }

    #[test]
    fn retirement_cutoff_counts_only_active_registered_entrants() {
        let mut s = fixture(192);
        s.retired = s.entrants.iter().take(47).map(|e| e.id.clone()).collect();
        // A historical, non-entrant identity must not change the denominator.
        s.retired.insert("historical-only".into());
        let threshold = RetirementThreshold::from_state(&s);
        assert_eq!(threshold.active_agents, 145);
        assert_eq!(threshold.probability, 0.01);
        s.retired.clear();
        assert_eq!(RetirementThreshold::from_state(&s).probability, 0.01);
        assert_eq!(
            RetirementThreshold::from_state(&fixture(500)).probability,
            0.01
        );
        assert_eq!(
            RetirementThreshold::from_state(&fixture(1000)).probability,
            0.005
        );
        assert_eq!(
            RetirementThreshold::from_state(&fixture(10000)).probability,
            0.0005
        );
        assert_eq!(
            RetirementThreshold::from_state(&fixture(20000)).probability,
            0.0005
        );
        assert_eq!(threshold.active_agents, 145);
        let empty = RetirementThreshold::from_state(&fixture(0));
        assert_eq!(empty.active_agents, 0);
        assert!(empty.probability.is_finite());
    }

    #[test]
    fn scaled_cutoff_keeps_borderline_agents_and_retains_lifecycle_guards() {
        let cutoff = RetirementThreshold::from_state(&fixture(1000)).probability;
        let mut e = Exposure {
            games: 40,
            opponents: 6,
            top_five_probability: 0.001,
            probability_upper: 0.007,
            protected_by: vec![],
        };
        assert!(inactive_under_policy(false, &e, true, 0.01));
        assert!(!inactive_under_policy(false, &e, true, cutoff));
        // Sampling uncertainty can retain inactivity even below the shared cutoff.
        assert!(inactive_under_policy(true, &e, true, cutoff));
        e.probability_upper = cutoff;
        assert!(!inactive_under_policy(false, &e, true, cutoff));
        e.top_five_probability = 0.;
        e.probability_upper = wilson_upper(0, DRAWS);
        assert!(inactive_under_policy(false, &e, true, cutoff));
        assert!(!inactive_under_policy(true, &e, false, cutoff));
        e.games = 39;
        assert!(!inactive_under_policy(true, &e, true, cutoff));
        e.games = 40;
        e.opponents = 5;
        assert!(!inactive_under_policy(true, &e, true, cutoff));
        e.opponents = 6;
        e.protected_by.push("generation:handcrafted".into());
        assert!(!inactive_under_policy(true, &e, true, cutoff));
        e.protected_by.clear();
        e.top_five_probability = cutoff + 0.001;
        e.probability_upper = cutoff + 0.002;
        assert!(!inactive_under_policy(true, &e, true, cutoff));
    }

    #[test]
    fn reactivation_uses_the_same_capped_cutoff_at_all_pool_sizes() {
        for active_agents in [145, 1000, 20000] {
            let threshold = RetirementThreshold::from_state(&fixture(active_agents));
            let cutoff = threshold.probability;
            assert_eq!(threshold.reactivation_probability, Some(cutoff));
            let mut e = Exposure {
                games: 40,
                opponents: 6,
                top_five_probability: cutoff * 1.1,
                probability_upper: cutoff * 1.2,
                protected_by: vec![],
            };
            // These probabilities are all far below the former 5% threshold.
            assert!(e.top_five_probability < 0.05);
            assert!(!inactive_under_policy(true, &e, true, cutoff));
            e.top_five_probability = cutoff;
            assert!(inactive_under_policy(true, &e, true, cutoff));
            e.top_five_probability = cutoff * 0.5;
            e.probability_upper = cutoff * 0.75;
            assert!(inactive_under_policy(false, &e, true, cutoff));
        }
    }

    #[test]
    fn legacy_checkpoint_inactivity_is_rechecked_once_and_cutoff_is_persisted() {
        let mut s = fixture(8);
        for a in 0..8 {
            for b in a + 1..8 {
                for seed in 0..8 {
                    result(&mut s, a, b, 0.5, seed);
                }
            }
        }
        s.retired.insert("agent07".into());
        refresh(&mut s).unwrap();
        assert!(s.retired.is_empty());
        let threshold = s.adaptive.as_ref().unwrap().retirement_threshold.unwrap();
        assert_eq!(threshold.active_agents, 7);
        assert_eq!(threshold.probability, 0.01);
        // Reusing unchanged evidence does not iterate the active-count dependency.
        refresh(&mut s).unwrap();
        assert_eq!(
            s.adaptive
                .as_ref()
                .unwrap()
                .retirement_threshold
                .unwrap()
                .active_agents,
            7
        );
        let saved = serde_json::to_value(&s).unwrap();
        let mut restored: TourneyState = serde_json::from_value(saved).unwrap();
        refresh(&mut restored).unwrap();
        assert_eq!(
            restored
                .adaptive
                .as_ref()
                .unwrap()
                .retirement_threshold
                .unwrap()
                .active_agents,
            7
        );
        result(&mut restored, 0, 1, 0.5, 100);
        refresh(&mut restored).unwrap();
        assert_eq!(
            restored
                .adaptive
                .unwrap()
                .retirement_threshold
                .unwrap()
                .active_agents,
            8
        );
        let fit = super::super::order_neutral::from_state(&s);
        assert_eq!(
            fit.retirement
                .unwrap()
                .adaptive_retirement_threshold
                .unwrap()
                .active_agents,
            7
        );
        // A v2 snapshot keeps its original denominator but gets the shared
        // reactivation rule on resume, even without new completed evidence.
        let mut v2 = serde_json::to_value(&s).unwrap();
        v2["adaptive"]["retirement_threshold"]
            .as_object_mut()
            .unwrap()
            .remove("reactivation_probability");
        v2["retired"] = serde_json::json!(["agent07"]);
        let mut restored: TourneyState = serde_json::from_value(v2).unwrap();
        assert_eq!(
            super::super::order_neutral::from_state(&restored)
                .retirement
                .unwrap()
                .policy,
            "adaptive-top5-p5overactive-clamped-p05-g40-o6-v2"
        );
        refresh(&mut restored).unwrap();
        assert!(restored.retired.is_empty());
        let applied = restored
            .adaptive
            .as_ref()
            .unwrap()
            .retirement_threshold
            .unwrap();
        assert_eq!(applied.active_agents, 7);
        assert_eq!(applied.reactivation_probability, Some(applied.probability));
        assert_eq!(
            super::super::order_neutral::from_state(&restored)
                .retirement
                .unwrap()
                .policy,
            "adaptive-top5-shared-p5overactive-clamped-g40-o6-v3"
        );
        // Check compatibility with pre-threshold scheduler JSON.
        let mut old = serde_json::to_value(&s).unwrap();
        old["adaptive"]
            .as_object_mut()
            .unwrap()
            .remove("retirement_threshold");
        let restored: TourneyState = serde_json::from_value(old).unwrap();
        assert!(restored.adaptive.unwrap().retirement_threshold.is_none());
    }

    #[test]
    fn migration_archives_unstarted_assignments_and_rollback_keeps_evidence() {
        let mut s = fixture(4);
        claim(&mut s).unwrap();
        claim(&mut s).unwrap();
        claim(&mut s).unwrap();
        s.slots[0].status = SlotStatus::Done;
        s.slots[0].score_a = Some(1.);
        for g in s.slots.iter_mut().skip(1) {
            g.status = SlotStatus::Pending;
        }
        s.format = TourneyFormat::Knockout;
        s.adaptive = None;
        let completed = serde_json::to_value(&s.slots[0]).unwrap();
        migrate(&mut s).unwrap();
        assert_eq!(s.slots[1].status, SlotStatus::Pending);
        assert_eq!(s.slots[2].status, SlotStatus::Cancelled);
        assert_eq!(s.slots[3].status, SlotStatus::Cancelled);
        assert_eq!(s.adaptive.as_ref().unwrap().archived_slots, vec![2, 3]);
        rollback(&mut s).unwrap();
        assert_eq!(s.format, TourneyFormat::Knockout);
        assert_eq!(serde_json::to_value(&s.slots[0]).unwrap(), completed);
        assert_eq!(
            claim_for_worker(&mut s, &TourneyConfig::default(), 0, false),
            Some(1)
        );
        rollback(&mut s).unwrap(); // The persisted flag permits normal crash recovery.
        s.slots[1].status = SlotStatus::Done;
        s.slots[1].score_a = Some(0.);
        assert!(claim_for_worker(&mut s, &TourneyConfig::default(), 0, false).is_some());
        assert!(!s.knockouts.is_empty());
    }

    #[test]
    fn migration_retries_interrupted_games_and_their_return_legs() {
        let mut s = fixture(4);
        for _ in 0..4 {
            claim(&mut s).unwrap();
        }
        for slot in &mut s.slots {
            slot.status = SlotStatus::Pending;
        }
        s.slots[0].status = SlotStatus::Aborted;
        s.format = TourneyFormat::Knockout;
        s.adaptive = None;
        migrate(&mut s).unwrap();
        assert_eq!(s.slots[0].status, SlotStatus::Aborted);
        assert_eq!(s.slots[1].status, SlotStatus::Pending);
        assert_eq!(s.slots[2].status, SlotStatus::Cancelled);
        assert_eq!(s.adaptive.as_ref().unwrap().completed_games, 0);
        assert!(s.slots.iter().all(|s| s.score_a.is_none()));
        // Resume converts interrupted attempts to pending before workers claim.
        s.slots[0].status = SlotStatus::Pending;
        assert_eq!(claim(&mut s).unwrap(), Some(0));
        assert_eq!(claim(&mut s).unwrap(), Some(1));
    }

    #[test]
    #[ignore = "50-seed adaptive-policy validation; run explicitly with --release --ignored --nocapture"]
    fn fifty_seed_policy_comparison() {
        use crate::training::tournament::{ensure_ratings, rate_finished_slot};
        use rand::{Rng, SeedableRng};
        let mut rows = vec![];
        let first_seed = std::env::var("SCHEDULER_SEED_START")
            .map(|s| s.parse::<u64>().expect("invalid simulation seed"))
            .unwrap_or(0);
        for scenario in ["sparse", "admission", "correlated"] {
            if std::env::var("SCHEDULER_SCENARIO").is_ok_and(|s| s != scenario) {
                continue;
            }
            for seed in first_seed..first_seed + 50 {
                for adaptive in [false, true] {
                    let mut s = fixture(12);
                    s.seed_base = seed + 1;
                    s.format = if adaptive {
                        TourneyFormat::Adaptive
                    } else {
                        TourneyFormat::Knockout
                    };
                    let strength = |i: usize| (((i * 7) % 12) as f64 - 5.5) * 0.35;
                    let mut truth: Vec<_> = (0..12).collect();
                    truth.sort_by(|&a, &b| strength(b).total_cmp(&strength(a)));
                    let true_top: BTreeSet<_> = truth
                        .into_iter()
                        .take(5)
                        .map(|i| format!("agent{i:02}"))
                        .collect();
                    let entrants = s.entrants.clone();
                    if scenario == "admission" {
                        s.entrants.truncate(8);
                    }
                    // Sparse initial evidence: two internally connected groups,
                    // with no cross-group results. These observations are shared
                    // by both policies and are excluded from the game budget.
                    for a in 0..s.entrants.len() {
                        let b = if a % 4 == 3 { a - 3 } else { a + 1 };
                        result(&mut s, a, b, 0.5, a as u64);
                    }
                    let initial = s.slots.len();
                    let cfg = TourneyConfig {
                        jobs: 8,
                        ..Default::default()
                    };
                    let mut rng = rand::rngs::StdRng::seed_from_u64(seed ^ 0x514c);
                    let mut workers = [None; 8];
                    let mut false_inactive = false;
                    for played in 0..320 {
                        if scenario == "admission" && played == 80 {
                            s.entrants = entrants.clone();
                        }
                        // Simulate game-boundary CPU withdrawal and restoration.
                        let capacity = if (120..200).contains(&played) { 2 } else { 8 };
                        for (worker, slot) in workers.iter_mut().enumerate() {
                            // The real eight-CPU gate retains workers 0 and 2.
                            if slot.is_none() && (capacity == 8 || worker == 0 || worker == 2) {
                                *slot = claim_for_worker(&mut s, &cfg, worker, !adaptive);
                            }
                        }
                        let busy: Vec<_> = workers
                            .iter()
                            .enumerate()
                            .filter_map(|(i, s)| s.map(|_| i))
                            .collect();
                        let worker = busy[rng.gen_range(0..busy.len())];
                        let id = workers[worker].take().unwrap();
                        let index = s.slots.iter().position(|g| g.id == id).unwrap();
                        let g = &s.slots[index];
                        let ai = g.model_a[5..].parse::<usize>().unwrap();
                        let bi = g.model_b[5..].parse::<usize>().unwrap();
                        let opening = if scenario == "correlated" {
                            if g.start_seed.is_multiple_of(2) {
                                0.65
                            } else {
                                -0.65
                            }
                        } else {
                            0.
                        };
                        let probability = model::sigmoid(
                            strength(ai) - strength(bi)
                                + if g.a_is_black { 0.15 } else { -0.15 }
                                + opening,
                        );
                        let draw = 0.08_f64.min(2. * probability.min(1. - probability));
                        let u = rng.gen::<f64>();
                        let score = if u < probability - draw / 2. {
                            1.
                        } else if u < probability + draw / 2. {
                            0.5
                        } else {
                            0.
                        };
                        let (a, b) = (g.model_a.clone(), g.model_b.clone());
                        s.slots[index].status = SlotStatus::Done;
                        s.slots[index].score_a = Some(score);
                        rate_finished_slot(&mut s, id, &a, &b, score);
                        false_inactive |= s.retired.iter().any(|i| true_top.contains(i));
                    }
                    ensure_ratings(&mut s);
                    let mut ranking: Vec<_> = s.ratings.iter().collect();
                    ranking.sort_by(|(a, x), (b, y)| y.r.total_cmp(&x.r).then(a.cmp(b)));
                    let recovered = ranking
                        .iter()
                        .take(5)
                        .filter(|(id, _)| true_top.contains(*id))
                        .count();
                    rows.push(serde_json::json!({"scenario":scenario,"seed":seed,"adaptive":adaptive,"games":320,"initial_games":initial,"top_five_recovered":recovered,"false_top_five_inactivity":false_inactive}));
                }
            }
        }
        println!(
            "SCHEDULER_SIMULATION {}",
            serde_json::to_string(&rows).unwrap()
        );
        for scenario in ["sparse", "admission", "correlated"] {
            for mode in [false, true] {
                let subset: Vec<_> = rows
                    .iter()
                    .filter(|r| r["scenario"] == scenario && r["adaptive"] == mode)
                    .collect();
                if subset.is_empty() {
                    continue;
                }
                let recovered: usize = subset
                    .iter()
                    .map(|r| r["top_five_recovered"].as_u64().unwrap() as usize)
                    .sum();
                let false_inactive = subset
                    .iter()
                    .filter(|r| r["false_top_five_inactivity"] == true)
                    .count();
                println!("{scenario} adaptive={mode} mean_top5={} false_inactive_runs={false_inactive}/50",recovered as f64/50.);
            }
        }
    }
}
