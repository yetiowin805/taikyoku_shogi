//! Equal-game-weight Bradley–Terry fit; draws contribute half a win each way.
//! No priors, temporal weights, or synthetic results. Ratings require a strongly
//! connected result graph and a converged likelihood fit, centered at 1500.
use super::tournament::{SlotStatus, TourneyState};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Fit {
    pub method: &'static str,
    pub status: &'static str,
    pub games: usize,
    pub components: Vec<Vec<String>>,
    pub component_edges: Vec<(usize, usize)>,
    pub ratings: BTreeMap<String, f64>,
    pub iterations: usize,
    pub max_score_residual: Option<f64>,
}

pub fn from_state(state: &TourneyState) -> Fit {
    let mut names: BTreeSet<String> = state.entrants.iter().map(|e| e.id.clone()).collect();
    let mut games = Vec::new();
    for slot in &state.slots {
        if slot.status != SlotStatus::Done {
            continue;
        }
        names.insert(slot.model_a.clone());
        names.insert(slot.model_b.clone());
        games.push((
            slot.model_a.clone(),
            slot.model_b.clone(),
            slot.score_a.unwrap_or(f64::NAN),
        ));
    }
    fit(&names.into_iter().collect::<Vec<_>>(), &games)
}

fn fit(names: &[String], games: &[(String, String, f64)]) -> Fit {
    let n = names.len();
    let mut out = Fit {
        method: "Bradley-Terry; equal game weights; draws=half; mean=1500",
        status: "disconnected",
        games: games.len(),
        components: vec![],
        component_edges: vec![],
        ratings: BTreeMap::new(),
        iterations: 0,
        max_score_residual: None,
    };
    let index: BTreeMap<_, _> = names.iter().enumerate().map(|(i, s)| (s, i)).collect();
    // Integer half-points make aggregation exactly independent of input order.
    let mut points = vec![vec![0u64; n]; n];
    for (a, b, score) in games {
        let (Some(&i), Some(&j)) = (index.get(a), index.get(b)) else {
            out.status = "invalid_results";
            return out;
        };
        if i == j || ![0.0, 0.5, 1.0].contains(score) {
            out.status = "invalid_results";
            return out;
        }
        let p = (score * 2.0) as u64;
        points[i][j] += p;
        points[j][i] += 2 - p;
    }
    let mut reach: Vec<Vec<bool>> = (0..n)
        .map(|i| (0..n).map(|j| i == j || points[i][j] > 0).collect())
        .collect();
    for k in 0..n {
        for i in 0..n {
            for j in 0..n {
                reach[i][j] |= reach[i][k] && reach[k][j];
            }
        }
    }
    let mut component = vec![usize::MAX; n];
    for i in 0..n {
        if component[i] != usize::MAX {
            continue;
        }
        let id = out.components.len();
        let mut group = Vec::new();
        for j in 0..n {
            if reach[i][j] && reach[j][i] {
                component[j] = id;
                group.push(names[j].clone());
            }
        }
        out.components.push(group);
    }
    let mut edges = BTreeSet::new();
    for i in 0..n {
        for j in 0..n {
            if points[i][j] > 0 && component[i] != component[j] {
                edges.insert((component[i], component[j]));
            }
        }
    }
    out.component_edges = edges.into_iter().collect();
    if n < 2 || out.components.len() != 1 {
        return out;
    }
    let pairs: Vec<_> = (0..n)
        .flat_map(|i| {
            (i + 1..n).filter_map({
                let points = &points;
                move |j| {
                    let count = (points[i][j] + points[j][i]) as f64 / 2.0;
                    (count > 0.0).then_some((i, j, count, points[i][j] as f64 / 2.0))
                }
            })
        })
        .collect();
    let loss = |x: &[f64]| -> f64 {
        pairs
            .iter()
            .map(|&(i, j, c, w)| {
                let d = x[i] - x[j];
                c * (d.max(0.0) + (-d.abs()).exp().ln_1p()) - w * d
            })
            .sum()
    };
    let mut x: Vec<f64> = vec![0.0; n];
    out.status = "not_converged";
    for iteration in 0..100 {
        out.iterations = iteration;
        let mut gradient = vec![0.0; n];
        let mut h = vec![vec![0.0; n - 1]; n - 1];
        for &(i, j, c, w) in &pairs {
            let p = 1.0 / (1.0 + (-(x[i] - x[j])).exp());
            let g = c * p - w;
            let v = c * p * (1.0 - p);
            gradient[i] += g;
            gradient[j] -= g;
            if i < n - 1 {
                h[i][i] += v;
            }
            if j < n - 1 {
                h[j][j] += v;
            }
            if i < n - 1 && j < n - 1 {
                h[i][j] -= v;
                h[j][i] -= v;
            }
        }
        let residual = gradient.iter().map(|g| g.abs()).fold(0.0, f64::max);
        out.max_score_residual = Some(residual);
        if residual < 1e-8 {
            let mean = x.iter().sum::<f64>() / n as f64;
            out.ratings = names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    (
                        name.clone(),
                        1500.0 + (x[i] - mean) * 400.0 / std::f64::consts::LN_10,
                    )
                })
                .collect();
            out.status = "converged";
            return out;
        }
        let Some(step) = solve(h, gradient[..n - 1].to_vec()) else {
            return out;
        };
        let current = loss(&x);
        let descent = gradient.iter().zip(&step).map(|(g, s)| g * s).sum::<f64>();
        let mut scale = 1.0;
        let mut accepted = false;
        for _ in 0..40 {
            let mut next = x.clone();
            for i in 0..n - 1 {
                next[i] -= scale * step[i];
            }
            if loss(&next) <= current - 1e-4 * scale * descent + 1e-12 {
                x = next;
                accepted = true;
                break;
            }
            scale *= 0.5;
        }
        if !accepted {
            return out;
        }
    }
    out
}

fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for k in 0..n {
        let pivot = (k..n).max_by(|&i, &j| a[i][k].abs().total_cmp(&a[j][k].abs()))?;
        if a[pivot][k].abs() < 1e-14 {
            return None;
        }
        a.swap(k, pivot);
        b.swap(k, pivot);
        for i in k + 1..n {
            let factor = a[i][k] / a[k][k];
            for j in k..n {
                a[i][j] -= factor * a[k][j];
            }
            b[i] -= factor * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (b[i] - (i + 1..n).map(|j| a[i][j] * x[j]).sum::<f64>()) / a[i][i];
    }
    x.iter().all(|v| v.is_finite()).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(g: &[(&str, &str, f64)], names: &[&str]) -> Fit {
        fit(
            &names.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &g.iter()
                .map(|&(a, b, s)| (a.into(), b.into(), s))
                .collect::<Vec<_>>(),
        )
    }
    #[test]
    fn analytic_ratio_draw_and_order_invariance() {
        let games = [
            ("a", "b", 1.0),
            ("a", "b", 1.0),
            ("a", "b", 1.0),
            ("a", "b", 0.0),
        ];
        let f = run(&games, &["a", "b"]);
        assert_eq!(f.status, "converged");
        assert!((f.ratings["a"] - f.ratings["b"] - 400.0 * 3f64.log10()).abs() < 1e-5);
        assert_eq!(
            f.ratings,
            run(&games.into_iter().rev().collect::<Vec<_>>(), &["a", "b"]).ratings
        );
        let d = run(&[("a", "b", 0.5)], &["a", "b"]);
        assert_eq!(d.ratings["a"], 1500.0);
    }
    #[test]
    fn strong_connectivity_not_just_each_agent_has_won_and_lost() {
        let f = run(
            &[("a", "b", 0.5), ("c", "d", 0.5), ("a", "c", 1.0)],
            &["a", "b", "c", "d"],
        );
        assert_eq!(f.status, "disconnected");
        assert!(f.ratings.is_empty());
        assert_eq!(f.components.len(), 2);
        assert_eq!(f.component_edges, vec![(0, 1)]);
        assert_eq!(run(&[("a", "b", 1.0)], &["a", "b"]).status, "disconnected");
        assert_eq!(
            run(&[("a", "b", 0.5)], &["a", "b", "unplayed"]).status,
            "disconnected"
        );
    }
    #[test]
    fn cycle_has_finite_fit_and_invalid_results_are_not_silently_draws() {
        let f = run(
            &[("a", "b", 1.0), ("b", "c", 1.0), ("c", "a", 1.0)],
            &["a", "b", "c"],
        );
        assert_eq!(f.status, "converged");
        assert!(f.ratings.values().all(|r| (*r - 1500.0).abs() < 1e-8));
        assert_eq!(
            run(&[("a", "b", f64::NAN)], &["a", "b"]).status,
            "invalid_results"
        );
    }
}
