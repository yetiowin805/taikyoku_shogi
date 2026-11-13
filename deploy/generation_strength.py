"""Offline hierarchical generation comparison; never writes tournament state.

Requires NumPyro, JAX and SciPy (analysis environment, not tournament runtime).
Snapshot supplies the authoritative generation map; --training can be older.
All strengths use natural-log odds internally and Elo differences in reports.
"""

import argparse
import hashlib
import json
import math
import os
import re
import time
from collections import Counter
from pathlib import Path

os.environ.setdefault("JAX_ENABLE_X64", "true")
os.environ.setdefault("OMP_NUM_THREADS", "2")
os.environ.setdefault("XLA_FLAGS", "--xla_cpu_multi_thread_eigen=false --xla_force_host_platform_device_count=4")

import jax
import jax.numpy as jnp
import numpy as np
import numpyro
import numpyro.distributions as dist
from jax.scipy.special import logsumexp
from numpyro.diagnostics import summary
from numpyro.infer import MCMC, NUTS, init_to_median
from scipy.special import logsumexp as scipy_logsumexp

SCALE = 400 / math.log(10)


def read_state(path):
    data = json.loads(Path(path).read_text())
    return data.get("state", data)


def block_key(slot):
    return (tuple(sorted([slot["model_a"], slot["model_b"]])), slot["start_seed"], slot["start_mode"])


def load_design(snapshot, training=None):
    payload = json.loads(Path(snapshot).read_text())
    state = payload["state"]
    names = sorted(e["id"] for e in state["entrants"])
    index = {name: i for i, name in enumerate(names)}
    members = {g: v["members"] for g, v in payload["fit"]["retirement"]["generations"].items()}
    membership = {name: g for g, agents in members.items() for name in agents}
    assert set(membership) == set(names)
    generations = sorted(members)
    # Share known parent networks and search recipes, regardless of generation
    # labels. A descriptor without a registered parent is an ordinary member.
    parents = {}
    for name in names:
        match = re.fullmatch(r"(NNUE_W\d+_v\d+(?:\.\d+)?[A-Za-z]*)_(.+)", name)
        if match and match[1] in index:
            parents[name] = (match[1], match[2])
    base_names = [n for n in names if n not in parents]
    base_generations = sorted({membership[n] for n in base_names})
    base_index = {name: i for i, name in enumerate(base_names)}
    variants = sorted(parents)
    waves = sorted({membership[n] for n in variants})
    recipes = sorted({parents[n][1] for n in variants})
    design = {
        "names": names,
        "generations": generations,
        "members": members,
        "group": np.array([generations.index(membership[n]) for n in names]),
        "base_global": np.array([index[n] for n in base_names], dtype=int),
        "base_group": np.array([base_generations.index(membership[n]) for n in base_names], dtype=int),
        "base_generations": base_generations,
        "variant_global": np.array([index[n] for n in variants], dtype=int),
        "variant_base": np.array([base_index[parents[n][0]] for n in variants], dtype=int),
        "variant_recipe": np.array([recipes.index(parents[n][1]) for n in variants], dtype=int),
        "variant_wave": np.array([waves.index(membership[n]) for n in variants], dtype=int),
        "waves": waves,
        "recipes": recipes,
        "anchor": 0,
    }
    train_state = read_state(training) if training else state
    train_done = [s for s in train_state["slots"] if s["status"] == "done"]
    current = {s["id"]: s for s in state["slots"] if s["status"] == "done"}
    for slot in train_done:
        new = current[slot["id"]]
        for field in ["model_a", "model_b", "start_seed", "start_mode", "a_is_black", "score_a"]:
            assert slot[field] == new[field], (slot["id"], field)
    return design, train_done, payload


def observations(slots, names):
    index = {name: i for i, name in enumerate(names)}
    grouped = {}
    for slot in slots:
        grouped.setdefault(block_key(slot), []).append(slot)
    keys = sorted(grouped)
    width = max(map(len, grouped.values()), default=1)
    arrays = {name: np.zeros((len(keys), width), dtype=int) for name in ["a", "b", "color", "y", "mask"]}
    slot_ids = np.full((len(keys), width), -1)
    for row, key in enumerate(keys):
        for col, slot in enumerate(sorted(grouped[key], key=lambda s: s["id"])):
            arrays["a"][row, col] = index[slot["model_a"]]
            arrays["b"][row, col] = index[slot["model_b"]]
            arrays["color"][row, col] = 1 if slot["a_is_black"] else -1
            arrays["y"][row, col] = {1.0: 0, 0.5: 1, 0.0: 2}[slot["score_a"]]
            arrays["mask"][row, col] = 1
            slot_ids[row, col] = slot["id"]
    return arrays, slot_ids


def quadrature(points):
    nodes, weights = np.polynomial.hermite.hermgauss(points)
    return nodes * math.sqrt(2), np.log(weights / math.sqrt(math.pi))


def model(design, obs, kind="structured", prior_scale=1.0, points=15, paired=True, search_prior_scale=1.0):
    count = len(design["names"])
    if kind == "structured":
        mu = numpyro.sample("mu_base", dist.Normal(0, 1.5 * prior_scale).expand([len(design["base_generations"])]))
        sd = numpyro.sample("sd_base", dist.HalfNormal(0.75 * prior_scale).expand([len(design["base_generations"])]))
        z = numpyro.sample("z_base", dist.Normal(0, 1).expand([len(design["base_global"])]))
        base = mu[design["base_group"]] + sd[design["base_group"]] * z
        delta = numpyro.sample("delta_wave", dist.Normal(0, 0.75 * prior_scale).expand([len(design["waves"])]))
        sd_recipe = numpyro.sample("sd_recipe", dist.HalfNormal(0.5 * prior_scale * search_prior_scale))
        recipe = sd_recipe * numpyro.sample("z_recipe", dist.Normal(0, 1).expand([len(design["recipes"])]))
        sd_interaction = numpyro.sample(
            "sd_interaction",
            dist.HalfNormal(0.35 * prior_scale * search_prior_scale).expand([len(design["waves"])]),
        )
        z_interaction = numpyro.sample("z_interaction", dist.Normal(0, 1).expand([len(design["variant_global"])]))
        variant = (
            base[design["variant_base"]]
            + delta[design["variant_wave"]]
            + recipe[design["variant_recipe"]]
            + sd_interaction[design["variant_wave"]] * z_interaction
        )
        theta = jnp.zeros(count).at[design["base_global"]].set(base).at[design["variant_global"]].set(variant)
    elif kind == "generation":
        mu = numpyro.sample("mu_generation", dist.Normal(0, 1.5 * prior_scale).expand([len(design["generations"])]))
        sd = numpyro.sample("sd_generation", dist.HalfNormal(0.75 * prior_scale).expand([len(design["generations"])]))
        z = numpyro.sample("z_agent", dist.Normal(0, 1).expand([count]))
        theta = mu[design["group"]] + sd[design["group"]] * z
    else:
        theta = numpyro.sample("theta_independent", dist.Normal(0, 4).expand([count]))
    # Anchor is a reporting transformation, not a constraint or an extra prior.
    theta = numpyro.deterministic("theta", theta - theta[design["anchor"]])
    black = numpyro.sample("black", dist.Normal(0, 1))
    log_nu = numpyro.sample("log_nu", dist.Normal(-2, 1.5))
    sigma_open = numpyro.sample("sigma_open", dist.HalfNormal(1)) if paired else 0.0
    nodes, weights = quadrature(points)
    d = (
        theta[obs["a"]][:, :, None]
        - theta[obs["b"]][:, :, None]
        + obs["color"][:, :, None] * (black + sigma_open * jnp.asarray(nodes))
    )
    logits = jnp.stack([d / 2, jnp.full_like(d, log_nu), -d / 2], axis=-1)
    probs = jax.nn.log_softmax(logits, axis=-1)
    selected = jnp.take_along_axis(probs, jnp.asarray(obs["y"])[:, :, None, None], axis=-1)[..., 0]
    pair_log = (selected * obs["mask"][:, :, None]).sum(axis=1)
    numpyro.factor("games", logsumexp(pair_log + jnp.asarray(weights), axis=1).sum())


def describe(x, weights=None):
    x = np.asarray(x)
    if weights is not None:
        x = x.reshape(-1)
        order = np.argsort(x)
        cumulative = np.cumsum(weights[order])
        quantiles = x[order[np.searchsorted(cumulative, [0.025, 0.5, 0.975]).clip(max=len(x) - 1)]]
        return {
            "mean": float(weights @ x),
            "median": float(quantiles[1]),
            "low": float(quantiles[0]),
            "high": float(quantiles[2]),
        }
    return {
        "mean": float(np.mean(x)),
        "median": float(np.median(x)),
        "low": float(np.quantile(x, 0.025)),
        "high": float(np.quantile(x, 0.975)),
    }


def expected_best(x, k):
    """Exact mean maximum of k distinct uniformly selected existing members."""
    n = x.shape[-1]
    weights = np.array([math.comb(r, k - 1) / math.comb(n, k) if r >= k - 1 else 0 for r in range(n)])
    return np.sort(x, axis=-1) @ weights


def report(samples, design, slots, payload, weights=None):
    strength = samples["theta"].reshape(-1, len(design["names"])) * SCALE
    strength = strength - strength.mean(axis=1, keepdims=True) + 1500
    averaging = np.full(len(strength), 1 / len(strength)) if weights is None else weights

    def describe_values(x):
        return describe(x, weights)

    equal_k = min(5, min(map(len, design["members"].values())))
    played = Counter(n for s in slots for n in [s["model_a"], s["model_b"]])
    active = set(design["names"]) - set(payload["state"]["retired"])
    groups = []
    maxima = []
    best5 = []
    for g in design["generations"]:
        indices = np.where(design["group"] == design["generations"].index(g))[0]
        x = strength[:, indices]
        champion = int(indices[np.argmax(averaging @ x)])
        maximum = x.max(axis=1)
        b5 = expected_best(x, equal_k)
        maxima.append(maximum)
        best5.append(b5)
        top = np.argmax(x, axis=1)
        groups.append(
            {
                "generation": g,
                "members": len(indices),
                "active": sum(design["names"][i] in active for i in indices),
                "games_per_agent": describe([played[design["names"][i]] for i in indices]),
                "champion": design["names"][champion],
                "champion_strength": describe_values(strength[:, champion]),
                "champion_games": played[design["names"][champion]],
                "champion_probability_best_in_generation": float(
                    averaging @ (top == np.where(indices == champion)[0][0])
                ),
                "strongest_existing": describe_values(maximum),
                "expected_best5": describe_values(b5),
                "roster_spread": describe_values(x.std(axis=1)),
                "best_k": {str(k): describe_values(expected_best(x, k)) for k in [1, 3, 5, 8] if k <= len(indices)},
            }
        )
    max_matrix = np.stack(maxima, axis=1)
    b5_matrix = np.stack(best5, axis=1)
    pairs = []
    for i, g in enumerate(design["generations"]):
        groups[i]["probability_strongest_generation"] = float(averaging @ (max_matrix.argmax(axis=1) == i))
        for j, h in enumerate(design["generations"]):
            if i >= j:
                continue
            a = max_matrix[:, i] - max_matrix[:, j]
            b = b5_matrix[:, i] - b5_matrix[:, j]
            pairs.append(
                {
                    "a": g,
                    "b": h,
                    "max_difference": describe_values(a),
                    "max_p_a_better": float(averaging @ (a > 0)),
                    "max_p_a_50_better": float(averaging @ (a > 50)),
                    "best5_difference": describe_values(b),
                    "best5_p_a_better": float(averaging @ (b > 0)),
                }
            )
    agents = [
        {
            "agent": name,
            "generation": design["generations"][design["group"][i]],
            "games": played[name],
            "strength": describe_values(strength[:, i]),
        }
        for i, name in enumerate(design["names"])
    ]
    agents.sort(key=lambda r: -r["strength"]["mean"])
    diagnostics = (
        summary(
            {
                "max": max_matrix.reshape(*samples["theta"].shape[:2], -1),
                "best5": b5_matrix.reshape(*samples["theta"].shape[:2], -1),
            },
            group_by_chain=True,
        )
        if weights is None
        else {}
    )
    return {
        "scale": "Registered field mean = 1500 in every posterior draw",
        "equal_k": equal_k,
        "games": len(slots),
        "generations": groups,
        "comparisons": pairs,
        "agents": agents,
        "black_advantage": describe_values(samples["black"] * SCALE),
        "opening_black_sd": describe_values(samples.get("sigma_open", np.zeros(samples["black"].shape)) * SCALE),
        "derived_diagnostics": {
            k: {"max_rhat": float(np.nanmax(v["r_hat"])), "min_ess": float(np.nanmin(v["n_eff"]))}
            for k, v in diagnostics.items()
        },
    }


def fit(args):
    design, slots, payload = load_design(args.snapshot, args.training)
    obs, _ = observations(slots, design["names"])
    start = time.monotonic()
    mcmc = MCMC(
        NUTS(
            model,
            target_accept_prob=args.accept,
            max_tree_depth=10,
            dense_mass=args.dense_mass,
            init_strategy=init_to_median(),
        ),
        num_warmup=args.warmup,
        num_samples=args.samples,
        num_chains=args.chains,
        chain_method="parallel",
        progress_bar=args.progress,
    )
    mcmc.run(
        jax.random.PRNGKey(args.seed),
        design,
        obs,
        args.kind,
        args.prior_scale,
        args.points,
        not args.independent_games,
        args.search_prior_scale,
        extra_fields=("diverging", "num_steps", "accept_prob"),
    )
    samples = {k: np.asarray(v) for k, v in mcmc.get_samples(group_by_chain=True).items()}
    extras = {k: np.asarray(v) for k, v in mcmc.get_extra_fields(group_by_chain=True).items()}
    latent = summary({k: v for k, v in samples.items() if k != "theta"}, group_by_chain=True)
    diagnostics = {
        k: {"max_rhat": float(np.nanmax(v["r_hat"])), "min_ess": float(np.nanmin(v["n_eff"]))}
        for k, v in latent.items()
    }
    output = Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    np.savez_compressed(output.with_suffix(".npz"), **samples)
    result = report(samples, design, slots, payload)
    result.update(
        {
            "arguments": vars(args),
            "seconds": time.monotonic() - start,
            "snapshot_sha256": hashlib.sha256(Path(args.snapshot).read_bytes()).hexdigest(),
            "training_sha256": hashlib.sha256(Path(args.training or args.snapshot).read_bytes()).hexdigest(),
            "diagnostics": diagnostics,
            "divergences": int(extras["diverging"].sum()),
            "max_tree_depth_hits": int((extras["num_steps"] >= 1023).sum()),
            "mean_acceptance": float(extras["accept_prob"].mean()),
            "mean_tree_steps": float(extras["num_steps"].mean()),
            "hyperparameters": {
                k: np.asarray(v).reshape(-1, *v.shape[2:]).mean(axis=0).tolist()
                for k, v in samples.items()
                if not k.startswith("z_") and k not in ["theta", "theta_independent"]
            },
        }
    )
    output.with_suffix(".json").write_text(json.dumps(result, indent=2) + "\n")
    print(
        json.dumps(
            {
                k: result[k]
                for k in [
                    "games",
                    "seconds",
                    "divergences",
                    "max_tree_depth_hits",
                    "derived_diagnostics",
                    "hyperparameters",
                ]
            }
        ),
        flush=True,
    )


def predictive(samples, obs, points=25, batch_size=100):
    """Marginal probabilities and joint block likelihood for unseen starts."""
    nodes, weights = quadrature(points)
    theta = samples["theta"].reshape(-1, samples["theta"].shape[-1])
    black = samples["black"].reshape(-1)
    sigma = samples.get("sigma_open", np.zeros_like(samples["black"])).reshape(-1)
    log_nu = samples["log_nu"].reshape(-1)
    prob_sum = np.zeros((*obs["a"].shape, 3))
    block_sum = np.zeros(obs["a"].shape[0])
    for start in range(0, len(theta), batch_size):
        end = start + batch_size
        d = (
            theta[start:end, obs["a"]][..., None]
            - theta[start:end, obs["b"]][..., None]
            + obs["color"][None, ..., None]
            * (black[start:end, None, None, None] + sigma[start:end, None, None, None] * nodes)
        )
        logits = np.stack([d / 2, np.broadcast_to(log_nu[start:end, None, None, None], d.shape), -d / 2], axis=-1)
        lp = logits - scipy_logsumexp(logits, axis=-1, keepdims=True)
        prob_sum += (np.exp(lp + weights[None, None, None, :, None]).sum(axis=-2)).sum(axis=0)
        selected = np.take_along_axis(lp, obs["y"][None, ..., None, None], axis=-1)[..., 0]
        block_sum += np.exp(
            scipy_logsumexp((selected * obs["mask"][None, ..., None]).sum(axis=2) + weights, axis=-1)
        ).sum(axis=0)
    return prob_sum / len(theta), np.log(block_sum / len(theta))


def validate(args):
    design, train, _ = load_design(args.snapshot, args.training)
    stop = read_state(args.until or args.snapshot)
    completed = [s for s in stop["slots"] if s["status"] == "done"]
    train_ids = {s["id"] for s in train}
    train_blocks = {block_key(s) for s in train}
    new = [s for s in completed if s["id"] not in train_ids]
    test = [s for s in new if block_key(s) not in train_blocks]
    obs, ids = observations(test, design["names"])
    same_generation = design["group"][obs["a"]] == design["group"][obs["b"]]
    categories = {"within_generation": same_generation, "between_generations": ~same_generation}
    results = []
    for path in args.fits:
        samples = dict(np.load(Path(path).with_suffix(".npz")))
        probabilities, block_log = predictive(samples, obs)
        selected = np.take_along_axis(probabilities, obs["y"][..., None], axis=-1)[..., 0]
        losses = -np.log(selected) * obs["mask"]
        one_hot = np.eye(3)[obs["y"]]
        brier = ((probabilities - one_hot) ** 2).sum(axis=-1) * obs["mask"]
        actual_score = 1.0 - 0.5 * obs["y"]
        expected_score = probabilities[..., 0] + 0.5 * probabilities[..., 1]
        score_brier = (expected_score - actual_score) ** 2 * obs["mask"]
        results.append(
            {
                "fit": str(path),
                "games": len(test),
                "blocks": len(ids),
                "excluded_partial_block_games": len(new) - len(test),
                "log_loss": float(losses.sum() / len(test)),
                "brier_3class": float(brier.sum() / len(test)),
                "score_brier": float(score_brier.sum() / len(test)),
                "joint_block_log_loss_per_game": float(-block_log.sum() / len(test)),
                "block_loss_sums": losses.sum(axis=1).tolist(),
                "block_game_counts": obs["mask"].sum(axis=1).tolist(),
                "block_score_loss_sums": score_brier.sum(axis=1).tolist(),
                "slot_ids": ids.tolist(),
                "categories": {
                    name: {
                        "games": int((mask * obs["mask"]).sum()),
                        "log_loss": float((losses * mask).sum() / (mask * obs["mask"]).sum()),
                        "brier_3class": float((brier * mask).sum() / (mask * obs["mask"]).sum()),
                        "score_brier": float((score_brier * mask).sum() / (mask * obs["mask"]).sum()),
                        "block_loss_sums": (losses * mask).sum(axis=1).tolist(),
                        "block_game_counts": (obs["mask"] * mask).sum(axis=1).tolist(),
                        "block_score_loss_sums": (score_brier * mask).sum(axis=1).tolist(),
                    }
                    for name, mask in categories.items()
                    if (mask * obs["mask"]).sum()
                },
            }
        )
    Path(args.output).write_text(json.dumps(results, indent=2) + "\n")
    print(
        json.dumps(
            [
                {
                    k: v
                    for k, v in r.items()
                    if k
                    not in ["block_loss_sums", "block_score_loss_sums", "block_game_counts", "slot_ids", "categories"]
                }
                for r in results
            ]
        ),
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    fitting = sub.add_parser("fit")
    fitting.add_argument("--snapshot", required=True)
    fitting.add_argument("--training")
    fitting.add_argument("--output", required=True)
    fitting.add_argument("--kind", choices=["structured", "generation", "independent"], default="structured")
    fitting.add_argument("--prior-scale", type=float, default=1.0)
    fitting.add_argument("--search-prior-scale", type=float, default=1.0)
    fitting.add_argument("--chains", type=int, default=4)
    fitting.add_argument("--warmup", type=int, default=500)
    fitting.add_argument("--samples", type=int, default=750)
    fitting.add_argument("--points", type=int, default=15)
    fitting.add_argument("--accept", type=float, default=0.9)
    fitting.add_argument("--seed", type=int, default=20261112)
    fitting.add_argument("--independent-games", action="store_true")
    fitting.add_argument("--dense-mass", action="store_true")
    fitting.add_argument("--progress", action="store_true")
    validation = sub.add_parser("validate")
    validation.add_argument("--snapshot", required=True)
    validation.add_argument("--training", required=True)
    validation.add_argument("--until")
    validation.add_argument("--output", required=True)
    validation.add_argument("--fits", nargs="+", required=True)
    args = parser.parse_args()
    if args.command == "fit":
        fit(args)
    else:
        validate(args)


if __name__ == "__main__":
    main()
