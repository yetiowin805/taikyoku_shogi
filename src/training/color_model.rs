//! Joint strength/color likelihood on within-SCC games. One shared Black bonus.
use super::order_neutral::{solve, Fit};
use super::tournament::{SlotStartMode, SlotStatus, TourneyState};
use std::collections::BTreeMap;

pub(super) const SCALE: f64 = 400.0 / std::f64::consts::LN_10;
type Features = Vec<(usize, f64)>;
type Blocks = BTreeMap<(String, String, u64, bool), [(usize, u64); 2]>;

pub(super) struct Model {
    index: BTreeMap<String, usize>,
    component: BTreeMap<String, usize>,
    pub beta: Vec<f64>,
    observations: Vec<(Features, usize, u64)>,
    pub games: usize,
    color_estimated: bool,
    pub status: &'static str,
}

impl Model {
    pub fn new(state: &TourneyState, fitted: &Fit) -> Result<Self, String> {
        let names: Vec<_> = fitted
            .components
            .iter()
            .flat_map(|g| &g[..g.len() - 1])
            .collect();
        let index = names
            .into_iter()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect::<BTreeMap<_, _>>();
        let component = fitted
            .components
            .iter()
            .enumerate()
            .flat_map(|(i, g)| g.iter().map(move |n| (n.clone(), i)))
            .collect();
        let mut model = Self {
            beta: vec![0.; index.len() + 1],
            index,
            component,
            observations: vec![],
            games: 0,
            color_estimated: matches!(fitted.color_fit_status, "not_fitted" | "estimated"),
            status: "not_fitted",
        };
        let mut counts = BTreeMap::<(String, String, bool), (usize, u64)>::new();
        for slot in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
            let score = slot.score_a.ok_or("completed slot missing score")?;
            if ![0., 0.5, 1.].contains(&score) || slot.model_a == slot.model_b {
                return Err("invalid completed result".into());
            }
            if model.component[&slot.model_a] != model.component[&slot.model_b] {
                continue;
            }
            let (a, b, black, score) = if slot.model_a < slot.model_b {
                (&slot.model_a, &slot.model_b, slot.a_is_black, score)
            } else {
                (&slot.model_b, &slot.model_a, !slot.a_is_black, 1. - score)
            };
            let entry = counts.entry((a.clone(), b.clone(), black)).or_default();
            entry.0 += 1;
            entry.1 += (2. * score) as u64;
            model.games += 1;
        }
        for ((a, b, black), (n, points)) in counts {
            model
                .observations
                .push((model.features(&a, &b, black), n, points));
        }
        Ok(model)
    }

    pub fn features(&self, a: &str, b: &str, black: bool) -> Features {
        let mut features = self.contrast(a, b);
        if self.color_estimated {
            features.push((self.beta.len() - 1, if black { 1. } else { -1. }));
        }
        features
    }

    pub fn contrast(&self, a: &str, b: &str) -> Features {
        [(a, 1.), (b, -1.)]
            .into_iter()
            .filter_map(|(name, sign)| self.index.get(name).map(|&i| (i, sign)))
            .collect()
    }

    fn logit(beta: &[f64], x: &Features) -> f64 {
        x.iter().map(|&(i, value)| beta[i] * value).sum()
    }

    fn loss(&self, beta: &[f64]) -> f64 {
        self.observations
            .iter()
            .map(|(x, n, points)| {
                let d = Self::logit(beta, x);
                *n as f64 * (d.max(0.) + (-d.abs()).exp().ln_1p()) - *points as f64 / 2. * d
            })
            .sum::<f64>()
    }

    pub fn information(&self) -> Vec<Vec<f64>> {
        let mut h = vec![vec![0.; self.beta.len()]; self.beta.len()];
        for (x, n, _) in &self.observations {
            let p = 1. / (1. + (-Self::logit(&self.beta, x)).exp());
            for &(i, a) in x {
                for &(j, b) in x {
                    h[i][j] += *n as f64 * p * (1. - p) * a * b;
                }
            }
        }
        let color = self.beta.len() - 1;
        if !self.color_estimated {
            h[color][color] = 1.;
        }
        h
    }

    pub fn estimate(&mut self) -> Result<(usize, f64), String> {
        let black_points: u64 = self
            .observations
            .iter()
            .map(|(x, n, points)| {
                if x.last()
                    .is_some_and(|&(i, sign)| i == self.beta.len() - 1 && sign > 0.)
                {
                    *points
                } else {
                    2 * *n as u64 - points
                }
            })
            .sum();
        self.status = if self.games == 0 {
            "no_within_scc_games"
        } else if black_points == 2 * self.games as u64 {
            "all_black_wins"
        } else if black_points == 0 {
            "all_white_wins"
        } else {
            let mut unit = vec![0.; self.beta.len()];
            *unit.last_mut().unwrap() = 1.;
            if solve(self.information(), unit).is_none() {
                "unidentifiable"
            } else {
                "estimated"
            }
        };
        if self.status == "estimated" {
            if let Ok(result) = self.optimize() {
                return Ok(result);
            }
            self.status = "no_finite_color_fit";
        }
        // Fix the color parameter at zero, then refit strengths using the
        // original likelihood. No synthetic games, priors, or strength penalty.
        self.color_estimated = false;
        self.beta.fill(0.);
        let color = self.beta.len() - 1;
        for (x, _, _) in &mut self.observations {
            x.retain(|&(i, _)| i != color);
        }
        self.optimize()
    }

    pub fn optimize(&mut self) -> Result<(usize, f64), String> {
        for iteration in 0..100 {
            let mut gradient = vec![0.; self.beta.len()];
            for (x, n, points) in &self.observations {
                let p = 1. / (1. + (-Self::logit(&self.beta, x)).exp());
                let residual = *n as f64 * p - *points as f64 / 2.;
                for &(i, value) in x {
                    gradient[i] += residual * value;
                }
            }
            let residual = gradient.iter().map(|g| g.abs()).fold(0., f64::max);
            if residual < 1e-8 {
                // A separating direction can have a tiny score gradient while
                // Newton still takes order-one steps toward infinity. Require
                // a stationary parameter estimate too, rather than publishing
                // a large iteration-dependent color/strength artifact.
                let stationary = !self.color_estimated
                    || solve(self.information(), gradient.clone())
                        .is_some_and(|step| step.iter().all(|s| s.abs() < 1e-5));
                if stationary {
                    return Ok((iteration, residual));
                }
            }
            let step =
                solve(self.information(), gradient.clone()).ok_or("singular joint color fit")?;
            let descent = gradient.iter().zip(&step).map(|(g, s)| g * s).sum::<f64>();
            let current = self.loss(&self.beta);
            let mut rate = 1.;
            let mut accepted = false;
            for _ in 0..40 {
                let next: Vec<_> = self
                    .beta
                    .iter()
                    .zip(&step)
                    .map(|(b, s)| b - rate * s)
                    .collect();
                if self.loss(&next) <= current - 1e-4 * rate * descent + 1e-12 {
                    self.beta = next;
                    accepted = true;
                    break;
                }
                rate *= 0.5;
            }
            if !accepted {
                return Err("joint color fit line search failed".into());
            }
        }
        Err("joint color fit did not converge".into())
    }

    pub fn set_from_fit(&mut self, fitted: &Fit) {
        for group in &fitted.components {
            let anchor = fitted.ratings[group.last().unwrap()];
            for name in &group[..group.len() - 1] {
                self.beta[self.index[name]] = (fitted.ratings[name] - anchor) / SCALE;
            }
        }
        *self.beta.last_mut().unwrap() = fitted.black_advantage_elo / SCALE;
    }

    pub fn ratings(&self, fitted: &Fit) -> BTreeMap<String, f64> {
        let mut ratings = BTreeMap::new();
        for group in &fitted.components {
            let value = |name: &String| self.index.get(name).map_or(0., |&i| self.beta[i] * SCALE);
            let mean = group.iter().map(value).sum::<f64>() / group.len() as f64;
            for name in group {
                ratings.insert(name.clone(), 1500. + value(name) - mean);
            }
        }
        ratings
    }

    pub fn covariance(&self) -> Result<Vec<Vec<f64>>, String> {
        let h = self.information();
        let n = self.beta.len();
        let mut covariance = vec![vec![0.; n]; n];
        for j in 0..n {
            let mut unit = vec![0.; n];
            unit[j] = 1.;
            let column =
                solve(h.clone(), unit).ok_or("singular color-aware retirement information")?;
            for (row, value) in covariance.iter_mut().zip(column) {
                row[j] = value;
            }
        }
        Ok(covariance)
    }

    pub fn cluster_meat(&self, state: &TourneyState) -> Vec<Vec<f64>> {
        let mut blocks = Blocks::new();
        for slot in state.slots.iter().filter(|s| s.status == SlotStatus::Done) {
            if self.component[&slot.model_a] != self.component[&slot.model_b] {
                continue;
            }
            let (a, b, black, score) = if slot.model_a < slot.model_b {
                (
                    &slot.model_a,
                    &slot.model_b,
                    slot.a_is_black,
                    slot.score_a.unwrap(),
                )
            } else {
                (
                    &slot.model_b,
                    &slot.model_a,
                    !slot.a_is_black,
                    1. - slot.score_a.unwrap(),
                )
            };
            let entry = &mut blocks
                .entry((
                    a.clone(),
                    b.clone(),
                    slot.start_seed,
                    slot.start_mode == SlotStartMode::Opening,
                ))
                .or_default()[usize::from(black)];
            entry.0 += 1;
            entry.1 += (score * 2.) as u64;
        }
        let mut meat = vec![vec![0.; self.beta.len()]; self.beta.len()];
        for ((a, b, _, _), colors) in &blocks {
            let mut gradient = vec![0.; self.beta.len()];
            for (black, &(count, points)) in colors.iter().enumerate() {
                let x = self.features(a, b, black == 1);
                let p = 1. / (1. + (-Self::logit(&self.beta, &x)).exp());
                let residual = points as f64 / 2. - count as f64 * p;
                for (i, value) in x {
                    gradient[i] += value * residual;
                }
            }
            for (i, row) in meat.iter_mut().enumerate() {
                for (j, value) in row.iter_mut().enumerate() {
                    *value += gradient[i] * gradient[j];
                }
            }
        }
        let p = self.beta.len() - usize::from(!self.color_estimated);
        if blocks.len() > 1 && self.games > p {
            let correction = blocks.len() as f64 / (blocks.len() - 1) as f64
                * (self.games - 1) as f64
                / (self.games - p) as f64;
            for row in &mut meat {
                for value in row {
                    *value *= correction;
                }
            }
        }
        meat
    }
}

pub(super) fn variance(covariance: &[Vec<f64>], contrast: &Features) -> f64 {
    contrast
        .iter()
        .map(|&(i, a)| {
            contrast
                .iter()
                .map(|&(j, b)| a * covariance[i][j] * b)
                .sum::<f64>()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::order_neutral::from_state;
    use crate::training::tournament::{build_schedule, TourneyConfig, TourneyEntrant, TourneySlot};

    fn fixture(black: (usize, usize), white: (usize, usize)) -> TourneyState {
        let mut state = build_schedule(&TourneyConfig {
            entrants: ["a", "b"]
                .iter()
                .map(|name| TourneyEntrant {
                    id: (*name).into(),
                    model: "unused.json".into(),
                    engine: None,
                    generation: None,
                })
                .collect(),
            ..Default::default()
        });
        state.slots.clear();
        for (a_is_black, (wins, games)) in [(true, black), (false, white)] {
            for game in 0..games {
                let id = state.slots.len();
                state.slots.push(TourneySlot {
                    id,
                    model_a: "a".into(),
                    model_b: "b".into(),
                    start_seed: id as u64,
                    a_is_black,
                    status: SlotStatus::Done,
                    game_path: None,
                    score_a: Some(f64::from(game < wins)),
                    round: 0,
                    start_mode: SlotStartMode::Opening,
                });
            }
        }
        state
    }

    #[test]
    fn joint_fit_recovers_strength_and_pooled_color_effect() {
        let mut state = fixture((300, 400), (200, 400));
        let fit = from_state(&state);
        let expected = SCALE * 3f64.ln() / 2.;
        assert!((fit.black_advantage_elo - expected).abs() < 1.);
        assert!((fit.ratings["a"] - fit.ratings["b"] - expected).abs() < 1.);
        // Start types never enter the design or create separate parameters.
        for slot in &mut state.slots {
            slot.start_mode = SlotStartMode::Light;
        }
        assert_eq!(
            fit.black_advantage_elo,
            from_state(&state).black_advantage_elo
        );
        state.slots.reverse();
        for slot in &mut state.slots {
            std::mem::swap(&mut slot.model_a, &mut slot.model_b);
            slot.score_a = slot.score_a.map(|score| 1. - score);
            slot.a_is_black = !slot.a_is_black;
        }
        let reversed = from_state(&state);
        assert_eq!(fit.ratings, reversed.ratings);
        assert_eq!(fit.black_advantage_elo, reversed.black_advantage_elo);
    }

    #[test]
    fn all_black_wins_and_unplayed_fields_remain_finite() {
        let state = fixture((20, 20), (0, 20));
        let fit = from_state(&state);
        assert_eq!(fit.black_advantage_elo, 0.);
        assert_eq!(fit.color_fit_status, "all_black_wins");
        let white = from_state(&fixture((0, 20), (20, 20)));
        assert_eq!(white.black_advantage_elo, 0.);
        assert_eq!(white.color_fit_status, "all_white_wins");
        // Both colors have wins overall, but one color/orientation cell still
        // perfectly separates: a finite joint MLE does not exist.
        let separated = from_state(&fixture((20, 20), (10, 20)));
        assert_eq!(separated.black_advantage_elo, 0.);
        assert_eq!(separated.color_fit_status, "no_finite_color_fit");
        assert!((fit.ratings["a"] - fit.ratings["b"]).abs() < 1e-8);
        let empty = from_state(&fixture((0, 0), (0, 0)));
        assert_eq!(empty.black_advantage_elo, 0.);
        assert_eq!(empty.color_fit_games, 0);
        assert!(empty.ratings.values().all(|r| *r == 1500.));
    }

    #[test]
    fn color_uncertainty_is_included_in_strength_contrasts() {
        let state = fixture((25, 50), (25, 50));
        let fit = from_state(&state);
        let mut model = Model::new(&state, &fit).unwrap();
        model.set_from_fit(&fit);
        let balanced = variance(&model.covariance().unwrap(), &model.contrast("a", "b"));
        assert!((balanced - 0.04).abs() < 1e-10);
        let state = fixture((60, 80), (10, 20));
        let fit = from_state(&state);
        let mut model = Model::new(&state, &fit).unwrap();
        model.set_from_fit(&fit);
        let confounded = variance(&model.covariance().unwrap(), &model.contrast("a", "b"));
        assert!((confounded - (1. / 15. + 1. / 5.) / 4.).abs() < 1e-9);
        assert!(confounded > 1. / 20.);
        let fixed = from_state(&fixture((50, 100), (0, 0)));
        assert_eq!(fixed.color_fit_status, "unidentifiable");
        assert_eq!(fixed.black_advantage_elo, 0.);
    }
}
