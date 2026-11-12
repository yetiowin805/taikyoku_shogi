//! Scheduling-only, regularized strength/color fit. Never publishes fantasy results.
use super::tournament::{SlotStartMode, SlotStatus, TourneyState};
use rand::{Rng, SeedableRng};
use std::collections::{BTreeMap, BTreeSet};

pub const SCALE: f64 = 400. / std::f64::consts::LN_10;
pub type Matrix = Vec<Vec<f64>>;

#[derive(Debug, Clone)]
pub struct Posterior {
    pub names: Vec<String>,
    pub mean: Vec<f64>,
    pub covariance: Matrix,
    pub factor: Matrix,
    pub games: usize,
}

pub fn sigmoid(x: f64) -> f64 {
    if x >= 0. {
        1. / (1. + (-x).exp())
    } else {
        x.exp() / (1. + x.exp())
    }
}

pub fn cholesky(a: &Matrix) -> Result<Matrix, String> {
    let n = a.len();
    let mut l = vec![vec![0.; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let value = a[i][j] - (0..j).map(|k| l[i][k] * l[j][k]).sum::<f64>();
            if i == j {
                if !value.is_finite() || value <= 0. {
                    return Err("scheduling covariance is not positive definite".into());
                }
                l[i][j] = value.sqrt();
            } else {
                l[i][j] = value / l[j][j];
            }
        }
    }
    Ok(l)
}

fn solve(l: &Matrix, b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut x = b.to_vec();
    for i in 0..n {
        x[i] = (x[i] - (0..i).map(|j| l[i][j] * x[j]).sum::<f64>()) / l[i][i];
    }
    for i in (0..n).rev() {
        x[i] = (x[i] - (i + 1..n).map(|j| l[j][i] * x[j]).sum::<f64>()) / l[i][i];
    }
    x
}

#[derive(Clone)]
struct Observation {
    a: usize,
    b: usize,
    color: f64,
    count: usize,
    points: f64,
}

fn information(theta: &[f64], observations: &[Observation]) -> (Matrix, Vec<f64>) {
    let n = theta.len();
    let mut h = vec![vec![0.; n]; n];
    let mut g = vec![0.; n];
    for i in 0..n {
        let precision = if i == n - 1 { 1. } else { 1. / 16. };
        h[i][i] = precision;
        g[i] = theta[i] * precision;
    }
    for o in observations {
        let p = sigmoid(theta[o.a] - theta[o.b] + o.color * theta[n - 1]);
        let residual = o.count as f64 * p - o.points;
        let w = o.count as f64 * p * (1. - p);
        let x = [(o.a, 1.), (o.b, -1.), (n - 1, o.color)];
        for (i, v) in x {
            g[i] += v * residual;
            for (j, u) in x {
                h[i][j] += v * u * w;
            }
        }
    }
    (h, g)
}

fn loss(theta: &[f64], observations: &[Observation]) -> f64 {
    let color = theta.len() - 1;
    let prior = theta
        .iter()
        .enumerate()
        .map(|(i, t)| t * t * if i == color { 0.5 } else { 1. / 32. })
        .sum::<f64>();
    prior
        + observations
            .iter()
            .map(|o| {
                let z = theta[o.a] - theta[o.b] + o.color * theta[color];
                o.count as f64 * (z.max(0.) + (-z.abs()).exp().ln_1p()) - o.points * z
            })
            .sum::<f64>()
}

pub fn fit(state: &TourneyState, assume_draws: bool) -> Result<Posterior, String> {
    let names: Vec<_> = state
        .entrants
        .iter()
        .map(|e| e.id.clone())
        .chain(
            state
                .slots
                .iter()
                .flat_map(|s| [s.model_a.clone(), s.model_b.clone()]),
        )
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let index: BTreeMap<_, _> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut aggregate = BTreeMap::<(usize, usize, bool), (usize, u64)>::new();
    let mut blocks = BTreeMap::<(usize, usize, u64, bool), Vec<(bool, f64)>>::new();
    let mut games = 0;
    for s in &state.slots {
        let score = match s.status {
            SlotStatus::Done => s.score_a.ok_or("finished game missing score")?,
            SlotStatus::Pending | SlotStatus::Running if assume_draws => 0.5,
            _ => continue,
        };
        if ![0., 0.5, 1.].contains(&score) || s.model_a == s.model_b {
            return Err("invalid scheduling result".into());
        }
        let (a, b, black, score) = if s.model_a < s.model_b {
            (
                index[s.model_a.as_str()],
                index[s.model_b.as_str()],
                s.a_is_black,
                score,
            )
        } else {
            (
                index[s.model_b.as_str()],
                index[s.model_a.as_str()],
                !s.a_is_black,
                1. - score,
            )
        };
        let entry = aggregate.entry((a, b, black)).or_default();
        entry.0 += 1;
        entry.1 += (score * 2.) as u64;
        blocks
            .entry((a, b, s.start_seed, s.start_mode == SlotStartMode::Opening))
            .or_default()
            .push((black, score));
        games += 1;
    }
    let observations: Vec<_> = aggregate
        .into_iter()
        .map(|((a, b, black), (count, points))| Observation {
            a,
            b,
            color: if black { 1. } else { -1. },
            count,
            points: points as f64 / 2.,
        })
        .collect();
    let n = names.len() + 1;
    let mut theta = vec![0.; n];
    let mut converged = false;
    for _ in 0..80 {
        let (h, g) = information(&theta, &observations);
        let step = solve(&cholesky(&h)?, &g);
        if step.iter().all(|s| s.abs() < 1e-8) {
            converged = true;
            break;
        }
        let before = loss(&theta, &observations);
        let descent: f64 = step.iter().zip(&g).map(|(a, b)| a * b).sum();
        let mut rate = 1.;
        let mut accepted = false;
        for _ in 0..40 {
            let next: Vec<_> = theta.iter().zip(&step).map(|(t, s)| t - rate * s).collect();
            if loss(&next, &observations) <= before - 1e-4 * rate * descent + 1e-10 {
                theta = next;
                accepted = true;
                break;
            }
            rate *= 0.5;
        }
        if !accepted {
            return Err("scheduling fit line search failed".into());
        }
    }
    if !converged {
        return Err("scheduling fit did not converge".into());
    }
    let (h, _) = information(&theta, &observations);
    let l = cholesky(&h)?;
    let mut covariance = vec![vec![0.; n]; n];
    for j in 0..n {
        let mut unit = vec![0.; n];
        unit[j] = 1.;
        for (row, value) in covariance.iter_mut().zip(solve(&l, &unit)) {
            row[j] = value;
        }
    }
    // Add a PSD cluster-sandwich term to the Laplace covariance. Unlike taking
    // elementwise maxima, this preserves a valid full joint covariance and is
    // conservative in every contrast direction (including poorly linked groups).
    let correction = if blocks.len() > 1 && games > n {
        blocks.len() as f64 / (blocks.len() - 1) as f64 * (games - 1) as f64 / (games - n) as f64
    } else {
        1.
    };
    let mut meat = vec![vec![0.; n]; n];
    for ((a, b, _, _), outcomes) in blocks {
        let mut residual = 0.;
        let mut color_residual = 0.;
        for (black, score) in outcomes {
            let color = if black { 1. } else { -1. };
            let r = score - sigmoid(theta[a] - theta[b] + color * theta[n - 1]);
            residual += r;
            color_residual += color * r;
        }
        for (i, x) in [(a, residual), (b, -residual), (n - 1, color_residual)] {
            for (j, y) in [(a, residual), (b, -residual), (n - 1, color_residual)] {
                meat[i][j] += correction * x * y;
            }
        }
    }
    let multiply = |a: &Matrix, b: &Matrix| {
        let mut out = vec![vec![0.; n]; n];
        for i in 0..n {
            for k in 0..n {
                let v = a[i][k];
                for (o, bv) in out[i].iter_mut().zip(&b[k]) {
                    *o += v * bv;
                }
            }
        }
        out
    };
    let robust = multiply(&multiply(&covariance, &meat), &covariance);
    for i in 0..n {
        for j in 0..n {
            covariance[i][j] += robust[i][j];
        }
    }
    let factor = cholesky(&covariance)?;
    Ok(Posterior {
        names,
        mean: theta,
        covariance,
        factor,
        games,
    })
}

impl Posterior {
    pub fn draws(&self, count: usize, seed: u64) -> Vec<Vec<f64>> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        (0..count)
            .map(|_| {
                let z: Vec<_> = (0..self.mean.len())
                    .map(|_| {
                        let u = rng.gen::<f64>().max(f64::MIN_POSITIVE);
                        (-2. * u.ln()).sqrt() * (std::f64::consts::TAU * rng.gen::<f64>()).cos()
                    })
                    .collect();
                self.mean
                    .iter()
                    .enumerate()
                    .map(|(i, m)| {
                        m + self.factor[i][..=i]
                            .iter()
                            .zip(&z)
                            .map(|(a, b)| a * b)
                            .sum::<f64>()
                    })
                    .collect()
            })
            .collect()
    }

    pub fn information_score(&self, a: usize, b: usize, draws: &[Vec<f64>]) -> f64 {
        let v =
            (self.covariance[a][a] + self.covariance[b][b] - 2. * self.covariance[a][b]).max(0.);
        v * self.outcome_information(a, b, draws)
    }

    pub fn outcome_information(&self, a: usize, b: usize, draws: &[Vec<f64>]) -> f64 {
        draws
            .iter()
            .map(|d| {
                [-1., 1.]
                    .iter()
                    .map(|c| {
                        let p = sigmoid(d[a] - d[b] + c * d[self.names.len()]);
                        p * (1. - p)
                    })
                    .sum::<f64>()
            })
            .sum::<f64>()
            / draws.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::training::tournament::{build_schedule, TourneyConfig, TourneyEntrant, TourneySlot};

    fn fixture() -> TourneyState {
        build_schedule(&TourneyConfig {
            format: crate::training::tournament::TourneyFormat::Adaptive,
            entrants: (0..4)
                .map(|i| TourneyEntrant {
                    id: format!("a{i}"),
                    model: "unused".into(),
                    engine: None,
                    generation: None,
                })
                .collect(),
            ..Default::default()
        })
    }

    #[test]
    fn common_prior_and_color_separation_preserve_unlinked_uncertainty() {
        let mut state = fixture();
        let prior = fit(&state, false).unwrap();
        assert_eq!(prior.mean, vec![0.; 5]);
        assert_eq!(prior.covariance[0][0], 16.);
        assert_eq!(prior.covariance[4][4], 1.);
        for i in 0..80 {
            let black = i % 2 == 0;
            state.slots.push(TourneySlot {
                id: i,
                model_a: "a0".into(),
                model_b: "a1".into(),
                start_seed: (i / 2) as u64,
                a_is_black: black,
                status: SlotStatus::Done,
                game_path: None,
                score_a: Some(if black { 1. } else { 0. }),
                round: 0,
                start_mode: SlotStartMode::Light,
            });
        }
        let p = fit(&state, false).unwrap();
        assert!((p.mean[0] - p.mean[1]).abs() < 1e-8);
        assert!(p.mean[4] > 2.);
        assert_eq!(p.covariance[2][2], 16.);
        assert_eq!(p.covariance[3][3], 16.);
        let contrast = p.covariance[0][0] + p.covariance[1][1] - 2. * p.covariance[0][1];
        assert!(contrast < 2.);
        assert!(p.covariance[0][0] + p.covariance[2][2] - 2. * p.covariance[0][2] > 20.);
    }

    #[test]
    fn rejects_invalid_completed_results_and_ignores_cancelled_work() {
        let mut state = fixture();
        state.slots.push(TourneySlot {
            id: 0,
            model_a: "a0".into(),
            model_b: "a1".into(),
            start_seed: 0,
            a_is_black: true,
            status: SlotStatus::Cancelled,
            game_path: None,
            score_a: None,
            round: 0,
            start_mode: SlotStartMode::Opening,
        });
        assert_eq!(fit(&state, true).unwrap().games, 0);
        state.slots[0].status = SlotStatus::Done;
        assert!(fit(&state, false).is_err());
        state.slots[0].score_a = Some(f64::NAN);
        assert!(fit(&state, false).is_err());
    }
}
