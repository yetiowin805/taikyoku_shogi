//! Fixed-center, SCC-local sandwich variance for retirement rank draws.
use super::{cholesky, fit_mean, sigmoid, solve, Matrix, Observation, Posterior, SCALE};
use crate::training::order_neutral::Fit;
use crate::training::tournament::{SlotStartMode, TourneyState};
use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::hash::{Hash, Hasher};

const FOLDS: usize = 5;
type Features = [(usize, f64); 3];

struct Game {
    x: Features,
    score: f64,
    weight: f64,
    prediction: f64,
}
fn probability(x: Features, theta: &[f64]) -> f64 {
    sigmoid(x.iter().map(|&(i, v)| v * theta[i]).sum())
}
fn outer(matrix: &mut Matrix, x: Features, y: Features, weight: f64) {
    for (i, a) in x {
        for (j, b) in y {
            matrix[i][j] += a * b * weight;
        }
    }
}

// Covariance when both ordinal scores are sampled from the same uniform
// quantile, or opposite quantiles. Mixing with independence preserves marginals.
fn paired_cov(a: f64, b: f64, draw_rate: f64, reverse: bool) -> f64 {
    let tails = |p: f64| {
        let half = draw_rate * p * (1. - p) / 2.;
        [p - half, p + half]
    };
    let mut joint = 0.;
    for u in tails(a) {
        for v in tails(b) {
            joint += if reverse {
                (u + v - 1.).max(0.)
            } else {
                u.min(v)
            };
        }
    }
    joint / 4. - a * b
}

pub(in crate::training) fn fit(state: &TourneyState, published: &Fit) -> Result<Posterior, String> {
    let mut names: Vec<_> = state.entrants.iter().map(|e| e.id.clone()).collect();
    names.sort();
    let n = names.len();
    let mut mean = names
        .iter()
        .map(|name| published.ratings[name] / SCALE)
        .collect::<Vec<_>>();
    mean.push(published.black_advantage_elo / SCALE);
    let mut games = Vec::new();
    let mut blocks = BTreeMap::new();
    let mut folds = vec![Vec::new(); FOLDS];
    let mut aggregates = vec![BTreeMap::new(); FOLDS];
    for slot in &state.slots {
        let first = slot.model_a < slot.model_b;
        let (a, b) = if first {
            (&slot.model_a, &slot.model_b)
        } else {
            (&slot.model_b, &slot.model_a)
        };
        let key = (
            a,
            b,
            slot.start_seed,
            slot.start_mode == SlotStartMode::Opening,
        );
        let mut hash = DefaultHasher::new();
        key.hash(&mut hash);
        let fold = (hash.finish() % FOLDS as u64) as usize;
        let score = slot
            .score_a
            .ok_or("missing completed result in retirement fit")?;
        let score = if first { score } else { 1. - score };
        let black = first == slot.a_is_black;
        let i = names
            .binary_search(a)
            .map_err(|_| "unknown retirement agent")?;
        let j = names
            .binary_search(b)
            .map_err(|_| "unknown retirement agent")?;
        let index = games.len();
        folds[fold].push(index);
        blocks.entry(key).or_insert_with(|| [vec![], vec![]])[usize::from(black)]
            .push((slot.id, index));
        let entry = aggregates[fold]
            .entry((i, j, black))
            .or_insert((0usize, 0.));
        entry.0 += 1;
        entry.1 += score;
        games.push(Game {
            x: [(i, 1.), (j, -1.), (n, if black { 1. } else { -1. })],
            score,
            weight: slot.rating_weight() as f64,
            prediction: 0.,
        });
    }
    for (held, indices) in folds.iter().enumerate() {
        if indices.is_empty() {
            continue;
        }
        let obs = aggregates
            .iter()
            .enumerate()
            .filter(|(k, _)| *k != held)
            .flat_map(|(_, agg)| {
                agg.iter()
                    .map(|(&(a, b, black), &(count, points))| Observation {
                        a,
                        b,
                        color: if black { 1. } else { -1. },
                        count,
                        points,
                    })
            })
            .collect::<Vec<_>>();
        let theta = fit_mean(&obs, &vec![0.25; n + 1])?;
        for &i in indices {
            games[i].prediction = probability(games[i].x, &theta);
        }
    }
    let draws = games.iter().filter(|g| g.score == 0.5).count() as f64;
    let draw_rate = (draws
        / games
            .iter()
            .map(|g| g.prediction * (1. - g.prediction))
            .sum::<f64>())
    .min(0.99);
    let mut pairs = Vec::new();
    for [mut black, mut white] in blocks.into_values() {
        black.sort_unstable();
        white.sort_unstable();
        pairs.extend(black.into_iter().zip(white).map(|((_, i), (_, j))| (i, j)));
    }
    let residual: f64 = pairs
        .iter()
        .map(|&(i, j)| {
            (games[i].score - games[i].prediction) * (games[j].score - games[j].prediction)
        })
        .sum();
    let reverse = residual < 0.;
    let bound: f64 = pairs
        .iter()
        .map(|&(i, j)| paired_cov(games[i].prediction, games[j].prediction, draw_rate, reverse))
        .sum();
    let mix = if bound.abs() > 1e-12 {
        // One independent pseudo-pair avoids singular score covariance when a
        // small panel happens to contain only identical sweeps or splits.
        (residual / bound).clamp(0., 1.) * pairs.len() as f64 / (pairs.len() + 1) as f64
    } else {
        0.
    };
    let zero = vec![vec![0.; n + 1]; n + 1];
    let mut h = zero.clone();
    let mut meat = zero.clone();
    for g in &games {
        let p = probability(g.x, &mean);
        outer(&mut h, g.x, g.x, g.weight * p * (1. - p));
        outer(
            &mut meat,
            g.x,
            g.x,
            g.weight.powi(2) * g.prediction * (1. - g.prediction) * (1. - draw_rate / 4.),
        );
    }
    for (i, j) in pairs {
        let (a, b) = (&games[i], &games[j]);
        let cov =
            mix * a.weight * b.weight * paired_cov(a.prediction, b.prediction, draw_rate, reverse);
        outer(&mut meat, a.x, b.x, cov);
        outer(&mut meat, b.x, a.x, cov);
    }
    // Add an arbitrary independent common rating offset; ranks and differences
    // are invariant to it. This makes both matrices invertible on that axis.
    for i in 0..n {
        for j in 0..n {
            h[i][j] += 1. / n as f64;
            meat[i][j] += 1. / n as f64;
        }
    }
    // Within a component color can be confounded even if identifiable globally.
    // Condition on the published color in that case, as for fixed-color ratings.
    if published.color_fit_status != "estimated" || cholesky(&h).is_err() {
        for matrix in [&mut h, &mut meat] {
            for i in 0..n {
                matrix[n][i] = 0.;
                matrix[i][n] = 0.;
            }
            matrix[n][n] = 1.;
        }
    }
    let l = cholesky(&h)?;
    // H^-1 J H^-1: solving the rows twice avoids explicitly inverting H.
    let left: Vec<_> = meat.iter().map(|row| solve(&l, row)).collect();
    let mut covariance = zero;
    for (j, row) in covariance.iter_mut().enumerate() {
        let column = left.iter().map(|row| row[j]).collect::<Vec<_>>();
        *row = solve(&l, &column);
    }
    let factor = cholesky(&covariance)?;
    Ok(Posterior {
        names,
        mean,
        covariance,
        factor,
        games: games.len(),
    })
}
