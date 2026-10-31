#!/usr/bin/env python3
"""Measure paired fixed-depth searches with exact signatures and bounded model memory.

Uses the existing JSONL server/harness. Run builds and tests before benchmarking;
the runner pins searches to one CPU but cannot isolate unrelated host activity.
"""

import argparse
import hashlib
import importlib.util
import json
import math
import os
import platform
import random
import shutil
import signal
import statistics
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "search_benchmark_harness", ROOT / "benchmarks/search_speed_20260920/run.py"
)
HARNESS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(HARNESS)


def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def verify_games(positions):
    # The helper eagerly replays every corpus root, even unselected cases.
    inputs = {}
    for entry in positions:
        path = entry["game"]
        if path not in inputs:
            inputs[path] = sha(path)
        if inputs[path] != entry["sha256"]:
            raise ValueError(f"game hash mismatch: {path}")
    return inputs


def cluster_interval(logs_by_cluster, bootstrap):
    if len(logs_by_cluster) < 2:
        return None
    totals = [(sum(values), len(values)) for values in logs_by_cluster.values()]
    rng = random.Random(20261030)
    samples = []
    for _ in range(bootstrap):
        selected = rng.choices(totals, k=len(totals))
        # Keep every row of each selected cluster. This retains the equal-case
        # estimand even when clusters contain different numbers of cases.
        samples.append(math.exp(sum(total for total, _ in selected) / sum(count for _, count in selected)))
    samples.sort()
    return [samples[int(bootstrap * 0.025)], samples[int(bootstrap * 0.975)]]


def summarize(rows, metric, positions, bootstrap=3000):
    logs = [math.log(r["measurements"]["candidate"][metric] / r["measurements"]["baseline"][metric]) for r in rows]
    # Multiple roots from one recorded game stay together in the primary
    # bootstrap. Position clustering remains a sensitivity calculation.
    position_logs = {}
    game_logs = {}
    for row, value in zip(rows, logs, strict=True):
        position_logs.setdefault(row["position"], []).append(value)
        position = positions[row["position"]]
        source_game = position.get("source_game") or position["game"]
        game_logs.setdefault(source_game, []).append(value)
    totals = {arm: sum(row["measurements"][arm][metric] for row in rows) for arm in ("baseline", "candidate")}
    summary = {
        "pairs": len(rows),
        "positions": len(position_logs),
        "source_games": len(game_logs),
        "metric": metric,
        "geomean_ms": {
            arm: math.exp(statistics.mean(math.log(r["measurements"][arm][metric]) for r in rows))
            for arm in ("baseline", "candidate")
        },
        "candidate_over_baseline": math.exp(statistics.mean(logs)),
        "total_ms": totals,
        "total_ratio": totals["candidate"] / totals["baseline"],
        "ratio_by_repetition": {
            str(rep): math.exp(
                statistics.mean(value for row, value in zip(rows, logs, strict=True) if row["rep"] == rep)
            )
            for rep in sorted({r["rep"] for r in rows})
        },
        "peak_rss_kib": {
            arm: max(row["measurements"][arm]["peak_rss_kib"] for row in rows) for arm in ("baseline", "candidate")
        },
        "total_nodes_each_arm": sum(row["measurements"]["baseline"]["nodes"] for row in rows),
        "q_nodes_each_arm": sum(row["measurements"]["baseline"]["q_nodes"] for row in rows),
        "nodes_include_q_nodes": True,
    }
    for name, clusters in (("source_game", game_logs), ("position", position_logs)):
        interval = cluster_interval(clusters, bootstrap)
        if interval is not None:
            summary[f"{name}_cluster_bootstrap_95"] = interval
    return summary


def run_stage(args, cases, corpus, stage, repeats, candidate):
    # Keep one model per pair of processes. Keeping every large network loaded
    # at once can create swapping and invalidate the comparison on laptops.
    HARNESS.OUT = args.out
    target = args.out / "bin/candidate"
    if target.is_symlink():
        target.unlink()
    target.symlink_to(candidate.resolve())
    rows = []
    rng = random.Random(args.seed)
    with (args.out / f"{stage}-pairs.jsonl").open("w") as raw, (args.out / f"{stage}-warmups.jsonl").open("w") as warm:
        for model in sorted({case["model"] for case in cases}):
            group = [case for case in cases if case["model"] == model]
            servers = {}
            try:
                for arm in ("baseline", "candidate"):
                    servers[arm] = HARNESS.Server(arm, f"{stage}-m{model}", args.cpu)
                for case in group:
                    result = {arm: servers[arm].request(case) for arm in servers}
                    HARNESS.compare(result["baseline"], result["candidate"])
                    warm.write(json.dumps({**case, "measurements": result}) + "\n")
                    warm.flush()
                starts = {case["position"]: rng.randrange(2) for case in group}
                for rep in range(repeats):
                    order = group.copy()
                    if not args.sequential:
                        rng.shuffle(order)
                    for case in order:
                        arms = ["baseline", "candidate"]
                        if (rep + starts[case["position"]]) % 2:
                            arms.reverse()
                        result = {arm: servers[arm].request(case) for arm in arms}
                        HARNESS.compare(result["baseline"], result["candidate"])
                        for value in result.values():
                            if value["completed_depth"] != case["depth"] or value["cpu_ms"] <= 0:
                                raise AssertionError("incomplete search or invalid CPU timer")
                        row = {**case, "rep": rep, "order": arms, "measurements": result}
                        raw.write(json.dumps(row) + "\n")
                        raw.flush()
                        rows.append(row)
                print(f"{stage}: {corpus['models'][model]['name']}: {len(group) * repeats} exact pairs", flush=True)
            finally:
                for server in servers.values():
                    server.close()
    summary = {
        "all_signatures_match": True,
        "cpu": summarize(rows, "cpu_ms", corpus["positions"]),
        "wall": summarize(rows, "ms", corpus["positions"]),
        "by_model": {
            corpus["models"][model]["name"]: summarize(
                [row for row in rows if row["model"] == model], "cpu_ms", corpus["positions"]
            )
            for model in sorted({row["model"] for row in rows})
        },
        "by_position": {
            corpus["positions"][position]["name"]: summarize(
                [row for row in rows if row["position"] == position], "cpu_ms", corpus["positions"]
            )
            for position in sorted({row["position"] for row in rows})
        },
    }
    (args.out / f"{stage}-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(f"{stage}: CPU ratio {summary['cpu']['candidate_over_baseline']:.5f}", flush=True)
    return summary


def terminate(signum, _frame):
    # SystemExit unwinds run_stage's finally blocks and reaps both workers.
    raise SystemExit(128 + signum)


def main():
    signal.signal(signal.SIGTERM, terminate)
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("baseline", "candidate", "corpus", "cases", "out"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--cpu", type=int, default=2)
    parser.add_argument("--repeats", type=int, default=4)
    parser.add_argument("--aa-repeats", type=int, default=0)
    parser.add_argument("--models", nargs="+", type=int)
    parser.add_argument("--positions", nargs="+", type=int)
    parser.add_argument("--sequential", action="store_true")
    parser.add_argument("--seed", type=int, default=20261030)
    parser.add_argument("--baseline-revision", required=True)
    parser.add_argument("--candidate-revision", required=True)
    args = parser.parse_args()
    if args.repeats < 1 or args.aa_repeats < 0 or args.cpu not in os.sched_getaffinity(0):
        parser.error("invalid repeat counts or CPU affinity")
    corpus = json.loads(args.corpus.read_text())
    cases = [
        {key: case[key] for key in ("position", "model", "depth")}
        for case in json.loads(args.cases.read_text())
        if (args.models is None or case["model"] in args.models)
        and (args.positions is None or case["position"] in args.positions)
    ]
    if not cases or len({(case["position"], case["model"]) for case in cases}) != len(cases):
        parser.error("cases must be nonempty with unique position/model pairs")
    inputs = verify_games(corpus["positions"])
    for model in {case["model"] for case in cases}:
        entry = corpus["models"][model]
        inputs[entry["path"]] = sha(entry["path"])
        if inputs[entry["path"]] != entry["sha256"]:
            raise ValueError(f"model hash mismatch: {entry['path']}")
        descriptor = json.loads(Path(entry["path"]).read_text())["weights"].get("nnue")
        if descriptor:
            blob = Path(entry["path"]).parent / descriptor["file"]
            inputs[str(blob)] = sha(blob)
            if inputs[str(blob)] != descriptor["sha256"]:
                raise ValueError(f"network hash mismatch: {blob}")
    args.out.mkdir(parents=True, exist_ok=False)
    (args.out / "bin").mkdir()
    (args.out / "bin/baseline").symlink_to(args.baseline.resolve())
    shutil.copyfile(args.corpus, args.out / "corpus.json")
    plan = {
        "cases": cases,
        "corpus": corpus,
        "inputs": inputs,
        "arguments": {key: str(value) if isinstance(value, Path) else value for key, value in vars(args).items()},
        "binaries": {
            arm: {"path": str(getattr(args, arm).resolve()), "sha256": sha(getattr(args, arm))}
            for arm in ("baseline", "candidate")
        },
        "compiler": subprocess.check_output(["rustc", "-Vv"], text=True),
        "runner_sha256": sha(__file__),
        "harness_sha256": sha(ROOT / "benchmarks/search_speed_20260920/run.py"),
        "cpu": subprocess.check_output(["lscpu"], text=True),
        "platform": platform.platform(),
        "python_version": platform.python_version(),
        "cpu_timer": "getrusage(RUSAGE_SELF): process user + system CPU milliseconds; see harness",
        "started_unix": time.time(),
        "load_before": os.getloadavg(),
        "method": "One pinned search at a time; persistent separately warmed processes per model; alternating arms (exactly balanced with even repeats); replay/loading/legal-route validation excluded; search setup included. CPU primary. Whole-source-game cluster bootstrap with equal-case weighting; position-cluster sensitivity; intervals omitted with fewer than two clusters. Nodes includes the q-node subset. This corpus is not a random sample; serial dependence and host interference remain possible. The inherited trusted JSONL harness watchdog waits for the first output byte and assumes a complete output line. No strength measurement.",
    }
    (args.out / "plan.json").write_text(json.dumps(plan, indent=2) + "\n")
    if args.aa_repeats:
        run_stage(args, cases, corpus, "aa", args.aa_repeats, args.baseline)
    run_stage(args, cases, corpus, "ab", args.repeats, args.candidate)


if __name__ == "__main__":
    main()
