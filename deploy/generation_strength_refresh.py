"""Refresh the local generation report; never mutate tournament state or ratings."""

import argparse
import fcntl
import hashlib
import json
import os
import subprocess
import sys
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path


def atomic(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2, allow_nan=False) + "\n")
    temporary.replace(path)


def evidence_key(payload):
    state = payload["state"]
    evidence = {
        "games": sorted(
            (g["id"], g["model_a"], g["model_b"], g["score_a"], g["a_is_black"], g["start_seed"], g["start_mode"])
            for g in state["slots"]
            if g["status"] == "done"
        ),
        "generations": {g: sorted(v["members"]) for g, v in payload["fit"]["retirement"]["generations"].items()},
        "retired": sorted(state["retired"]),
    }
    return hashlib.sha256(json.dumps(evidence, sort_keys=True).encode()).hexdigest()


@contextmanager
def queue_lock(work):
    with (work / "queue.lock").open("a+") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield


def alive(pid, work):
    try:
        command = Path(f"/proc/{pid}/cmdline").read_bytes()
        status = Path(f"/proc/{pid}/stat").read_text().split(") ", 1)[1]
        return "Z" != status[0] and b"generation_strength_refresh.py" in command and str(work).encode() in command
    except (OSError, IndexError):
        return False


def request(args):
    work = Path(args.work_dir).resolve()
    web = Path(args.web_dir).resolve()
    work.mkdir(parents=True, exist_ok=True)
    payload = json.loads(Path(args.snapshot).read_text())
    key = evidence_key(payload)
    with queue_lock(work):
        atomic(work / "requested.json", payload)
        config_path = work / "config.json"
        config = json.loads(config_path.read_text()) if config_path.exists() else {}
        if args.baseline:
            config.update(
                baseline=str(Path(args.baseline).resolve()),
                baseline_snapshot=str(Path(args.baseline_snapshot).resolve()),
            )
        config.update(web=str(web))
        atomic(config_path, config)
        result = web / "generation-strength-live.json"
        if result.exists() and json.loads(result.read_text()).get("evidence_key") == key:
            return
        worker_file = work / "worker.json"
        worker = json.loads(worker_file.read_text()) if worker_file.exists() else {}
        if alive(worker.get("pid"), work):
            return
        with (work / "worker.log").open("a") as log:
            proc = subprocess.Popen(
                [sys.executable, str(Path(__file__).resolve()), "work", "--work-dir", str(work)],
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
        atomic(worker_file, {"pid": proc.pid})
    print("Generation analysis queued; previous accepted estimates remain available.")


def block_log_likelihood(samples, obs, points=15):
    import numpy as np
    from scipy.special import logsumexp

    from generation_strength import quadrature

    nodes, weights = quadrature(points)
    theta = samples["theta"].reshape(-1, samples["theta"].shape[-1])
    black = samples["black"].reshape(-1)
    sigma = samples.get("sigma_open", np.zeros_like(samples["black"])).reshape(-1)
    nu = samples["log_nu"].reshape(-1)
    out = []
    for start in range(0, len(theta), 40):
        end = start + 40
        d = theta[start:end, obs["a"]][..., None] - theta[start:end, obs["b"]][..., None]
        d = d + obs["color"][None, ..., None] * (
            black[start:end, None, None, None] + sigma[start:end, None, None, None] * nodes
        )
        logits = np.stack([d / 2, np.broadcast_to(nu[start:end, None, None, None], d.shape), -d / 2], axis=-1)
        lp = logits - logsumexp(logits, axis=-1, keepdims=True)
        selected = np.take_along_axis(lp, obs["y"][None, ..., None, None], axis=-1)[..., 0]
        out.append(logsumexp((selected * obs["mask"][None, ..., None]).sum(axis=2) + weights, axis=-1).sum(axis=-1))
    return np.concatenate(out)


def importance(samples, baseline, current, design):
    import numpy as np
    from arviz_stats.base import array_stats
    from scipy.special import logsumexp

    from generation_strength import block_key, observations

    # Recompute the whole affected block, including any already-observed leg.
    # This handles a returning color leg, changed results, and removed games.
    before = {g["id"]: g for g in baseline}
    after = {g["id"]: g for g in current}
    fields = ["model_a", "model_b", "score_a", "a_is_black", "start_seed", "start_mode"]
    changed = {
        block_key(g)
        for i in set(before) | set(after)
        if i not in before or i not in after or any(before[i][f] != after[i][f] for f in fields)
        for g in [before.get(i), after.get(i)]
        if g is not None
    }
    logw = np.zeros(samples["theta"].shape[0] * samples["theta"].shape[1])
    for sign, games in [(1, current), (-1, baseline)]:
        subset = [g for g in games if block_key(g) in changed]
        if subset:
            obs, _ = observations(subset, design["names"])
            logw += sign * block_log_likelihood(samples, obs)
    if np.ptp(logw) < 1e-12:
        w, k = np.full(len(logw), 1 / len(logw)), 0.0
    else:
        smoothed, k = array_stats.psislw(logw, r_eff=0.5)
        w = np.exp(smoothed - logsumexp(smoothed))
    diagnostics = {
        "pareto_k": float(k) if np.isfinite(k) else None,
        "weight_ess": float(1 / np.square(w).sum()),
        "draws": len(w),
        "changed_blocks": len(changed),
    }
    accepted = np.isfinite(k) and k < 0.7 and diagnostics["weight_ess"] >= max(400, 0.1 * len(w))
    return w, diagnostics, bool(accepted)


def valid_fit(fit):
    diagnostics = list(fit["diagnostics"].values()) + list(fit["derived_diagnostics"].values())
    return (
        fit["divergences"] == 0
        and fit["max_tree_depth_hits"] == 0
        and all(d["max_rhat"] <= 1.05 and d["min_ess"] >= 200 for d in diagnostics)
    )


def compute(work, config, payload):
    import numpy as np

    from generation_strength import load_design, report

    web = Path(config["web"])
    snapshot = work / "fitting.json"
    atomic(snapshot, payload)
    design, slots, _ = load_design(snapshot)
    key = evidence_key(payload)

    def status(phase, **extra):
        atomic(
            web / "generation-strength-status.json",
            {
                "phase": phase,
                "requested_games": len(slots),
                "evidence_key": key,
                "updated_at": datetime.now(timezone.utc).isoformat(),
                **extra,
            },
        )

    status("updating")
    accepted = False
    weights = diagnostics = samples = None
    if config.get("baseline"):
        base_design, base_slots, _ = load_design(config["baseline_snapshot"])
        baseline_fit = json.loads(Path(config["baseline"]).with_suffix(".json").read_text())
        arguments = baseline_fit["arguments"]
        baseline_matches_model = (
            arguments["kind"] == "structured"
            and arguments["points"] == 15
            and arguments.get("prior_scale", 1) == 1
            and arguments.get("search_prior_scale", 1) == 1
            and not arguments.get("independent_games", False)
            and valid_fit(baseline_fit)
        )
        compatible = all(
            np.array_equal(design[k], base_design[k])
            for k in ["names", "group", "base_group", "variant_base", "variant_recipe", "variant_wave", "recipes"]
        )
        if compatible and baseline_matches_model:
            samples = dict(np.load(Path(config["baseline"]).with_suffix(".npz")))
            weights, diagnostics, accepted = importance(samples, base_slots, slots, design)
    method = "importance update"
    if not accepted:
        status("refitting", importance=diagnostics)
        # A fresh fit becomes the new baseline; no repeated chaining of weights.
        prefix = work / ("fit-" + key[:16])
        command = [
            sys.executable,
            str(Path(__file__).with_name("generation_strength.py")),
            "fit",
            "--snapshot",
            str(snapshot),
            "--output",
            str(prefix),
            "--points",
            "15",
            "--chains",
            "4",
            "--warmup",
            "600",
            "--samples",
            "1000",
        ]
        subprocess.run(command, check=True)
        fit = json.loads(prefix.with_suffix(".json").read_text())
        if not valid_fit(fit):
            raise RuntimeError("New fit failed convergence checks; keeping the previous report")
        saved_snapshot = prefix.with_suffix(".snapshot.json")
        atomic(saved_snapshot, payload)
        with queue_lock(work):
            current_config = json.loads((work / "config.json").read_text())
            current_config.update(baseline=str(prefix), baseline_snapshot=str(saved_snapshot))
            atomic(work / "config.json", current_config)
        samples = dict(np.load(prefix.with_suffix(".npz")))
        weights = None
        diagnostics = {
            "latent": fit["diagnostics"],
            "derived": fit["derived_diagnostics"],
            "divergences": fit["divergences"],
        }
        method = "full posterior fit"
    result = report(samples, design, slots, payload, weights)
    result.update(
        evidence_key=key,
        asof=payload["state"]["_checked_utc"],
        generated_at=datetime.now(timezone.utc).isoformat(),
        update_method=method,
        update_diagnostics=diagnostics,
    )
    atomic(web / "generation-strength-live.json", result)
    status("current", method=method)
    return key


def worker(args):
    work = Path(args.work_dir).resolve()
    os.nice(10)
    if hasattr(os, "sched_getaffinity"):
        os.sched_setaffinity(0, sorted(os.sched_getaffinity(0))[:2])
    while True:
        with queue_lock(work):
            payload = json.loads((work / "requested.json").read_text())
            config = json.loads((work / "config.json").read_text())
        try:
            processed = compute(work, config, payload)
        except Exception as exc:
            with queue_lock(work):
                atomic(
                    Path(config["web"]) / "generation-strength-status.json",
                    {"phase": "failed", "error": str(exc), "updated_at": datetime.now(timezone.utc).isoformat()},
                )
                atomic(work / "worker.json", {"pid": None})
            raise
        with queue_lock(work):
            latest = json.loads((work / "requested.json").read_text())
            if evidence_key(latest) == processed:
                atomic(work / "worker.json", {"pid": None})
                break


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    enqueue = sub.add_parser("request")
    enqueue.add_argument("--snapshot", required=True)
    enqueue.add_argument("--web-dir", required=True)
    enqueue.add_argument("--work-dir", required=True)
    enqueue.add_argument("--baseline")
    enqueue.add_argument("--baseline-snapshot")
    process = sub.add_parser("work")
    process.add_argument("--work-dir", required=True)
    args = parser.parse_args()
    if args.command == "request":
        if bool(args.baseline) != bool(args.baseline_snapshot):
            parser.error("--baseline and --baseline-snapshot must be supplied together")
        request(args)
    else:
        worker(args)


if __name__ == "__main__":
    main()
