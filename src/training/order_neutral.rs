//! Equal-game-weight Bradley–Terry fit; draws contribute half a win each way.
//! No temporal weighting or synthetic games. Separated SCCs use a provisional
//! minimum-adjustment embedding with 100-point gaps; disconnected groups center independently.
use super::tournament::{SlotStatus, TourneyState};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Serialize)]
pub struct Fit {
    pub method: &'static str,
    pub status: &'static str,
    pub games: usize,
    pub components: Vec<Vec<String>>,
    pub weak_components: Vec<Vec<String>>,
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
    let mut result = fit(&names.into_iter().collect::<Vec<_>>(), &games);
    // Retired opponents retain all their evidence, but the active field defines
    // the displayed center. Translation cannot alter any fitted difference.
    for group in &result.weak_components {
        let active: Vec<_> = group
            .iter()
            .filter(|id| !state.retired.contains(*id))
            .collect();
        if active.is_empty() {
            continue;
        }
        let values: Vec<_> = active
            .iter()
            .filter_map(|id| result.ratings.get(*id))
            .collect();
        if values.len() != active.len() {
            continue;
        }
        let shift = 1500.0 - values.iter().copied().sum::<f64>() / values.len() as f64;
        for id in group {
            if let Some(r) = result.ratings.get_mut(id) {
                *r += shift;
            }
        }
    }
    result
}

fn fit(names: &[String], games: &[(String, String, f64)]) -> Fit {
    let n = names.len();
    let mut out = Fit {
        method: "Bradley-Terry; equal game weights; draws=half; mean=1500",
        status: "disconnected",
        games: games.len(),
        components: vec![],
        weak_components: vec![],
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
    let mut remaining: BTreeSet<usize> = (0..n).collect();
    while let Some(&first) = remaining.iter().next() {
        remaining.remove(&first);
        let mut members = vec![first];
        let mut cursor = 0;
        while cursor < members.len() {
            let i = members[cursor];
            cursor += 1;
            let neighbors: Vec<_> = remaining
                .iter()
                .copied()
                .filter(|&j| points[i][j] + points[j][i] > 0)
                .collect();
            for j in neighbors {
                remaining.remove(&j);
                members.push(j);
            }
        }
        members.sort_unstable();
        out.weak_components
            .push(members.iter().map(|&i| names[i].clone()).collect());
    }
    if n == 1 {
        out.ratings.insert(names[0].clone(), 1500.0);
        out.status = "converged";
        return out;
    }
    if n == 0 {
        return out;
    }
    if out.components.len() != 1 {
        return separated_fit(out, games);
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

/// Weighted projection of zero shifts onto all DAG difference constraints.
/// Hildreth's dual coordinate ascent solves the strictly convex minimum-change
/// problem; deterministic edge order and half-point input preserve order neutrality.
fn separated_fit(mut out: Fit, games: &[(String, String, f64)]) -> Fit {
    for group in &out.components {
        let inner: Vec<_> = games
            .iter()
            .filter(|(a, b, _)| group.contains(a) && group.contains(b))
            .cloned()
            .collect();
        let fitted = fit(group, &inner);
        if fitted.status != "converged" {
            out.ratings.clear();
            out.status = "not_converged";
            return out;
        }
        out.ratings.extend(fitted.ratings);
    }
    let n = out.components.len();
    let weights: Vec<_> = out.components.iter().map(|g| g.len() as f64).collect();
    let edges: Vec<_> = out
        .component_edges
        .iter()
        .map(|&(a, b)| {
            let low = out.components[a]
                .iter()
                .map(|id| out.ratings[id])
                .fold(f64::INFINITY, f64::min);
            let high = out.components[b]
                .iter()
                .map(|id| out.ratings[id])
                .fold(f64::NEG_INFINITY, f64::max);
            (a, b, 100.0 + high - low)
        })
        .collect();
    let mut shifts = vec![0.; n];
    let mut lambda = vec![0.; edges.len()];
    let mut converged = edges.is_empty();
    for iteration in 0..100_000 {
        let mut change: f64 = 0.;
        for (e, &(a, b, gap)) in edges.iter().enumerate() {
            let step = ((gap - shifts[a] + shifts[b]) / (1. / weights[a] + 1. / weights[b]))
                .max(-lambda[e]);
            lambda[e] += step;
            shifts[a] += step / weights[a];
            shifts[b] -= step / weights[b];
            change = change.max(step.abs());
        }
        out.iterations = iteration;
        if change < 1e-8
            && edges
                .iter()
                .all(|&(a, b, gap)| shifts[a] - shifts[b] >= gap - 1e-7)
        {
            converged = true;
            break;
        }
    }
    if !converged {
        out.ratings.clear();
        out.status = "not_converged";
        return out;
    }
    for (i, group) in out.components.iter().enumerate() {
        for id in group {
            *out.ratings.get_mut(id).unwrap() += shifts[i];
        }
    }
    out.method = "Bradley-Terry within SCCs; minimum squared agent shifts; edge gap >=100; active mean=1500 per connected group";
    out.status = if out.weak_components.len() > 1 {
        "disconnected"
    } else {
        "separated"
    };
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
        assert_eq!(f.status, "separated");
        assert_eq!(f.ratings.len(), 4);
        assert_eq!(f.components.len(), 2);
        assert_eq!(f.component_edges, vec![(0, 1)]);
        assert_eq!(run(&[("a", "b", 1.0)], &["a", "b"]).status, "separated");
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
    #[test]
    fn provisional_dag_preserves_internal_fit_and_centers_disconnected_groups() {
        let g = [
            ("a", "b", 1.),
            ("a", "b", 0.5),
            ("b", "c", 1.),
            ("b", "d", 1.),
        ];
        let f = run(&g, &["a", "b", "c", "d", "new"]);
        assert_eq!(f.status, "disconnected");
        assert_eq!(f.weak_components.len(), 2);
        assert_eq!(f.ratings["new"], 1500.);
        assert!((f.ratings["c"] - f.ratings["d"]).abs() < 1e-7);
        assert!(f.ratings["b"] - f.ratings["c"] >= 100. - 1e-6);
        assert!((f.ratings["a"] - f.ratings["b"] - 400. * 3f64.log10()).abs() < 1e-6);
        assert!((f.ratings.values().sum::<f64>() / 5. - 1500.).abs() < 1e-7);
        let mut reversed = g.to_vec();
        reversed.reverse();
        assert_eq!(
            f.ratings,
            run(&reversed, &["a", "b", "c", "d", "new"]).ratings
        );
        let chain = run(&[("a", "b", 1.), ("b", "c", 1.)], &["a", "b", "c"]);
        for (id, wanted) in [("a", 1600.), ("b", 1500.), ("c", 1400.)] {
            assert!((chain.ratings[id] - wanted).abs() < 1e-6);
        }
    }
}
