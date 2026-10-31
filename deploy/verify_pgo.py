#!/usr/bin/env python3
"""Compare same-source native/PGO searches before selecting a tournament build."""

import argparse
import contextlib
import json
import math
import os
import random
import signal
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "benchmarks/nnue_followup"))
import profile_workload as workload


def summarize(rows, corpus):
    def metrics(selected):
        ratios = [row["candidate"]["cpu_ms"] / row["baseline"]["cpu_ms"] for row in selected]
        return dict(
            pairs=len(selected),
            candidate_over_baseline=math.exp(sum(map(math.log, ratios)) / len(ratios)),
            geomean_cpu_ms={
                arm: math.exp(sum(math.log(row[arm]["cpu_ms"]) for row in selected) / len(selected))
                for arm in ("baseline", "candidate")
            },
            summed_cpu_ms={arm: sum(row[arm]["cpu_ms"] for row in selected) for arm in ("baseline", "candidate")},
        )

    result = metrics(rows)
    games = {entry["game"] for entry in corpus["positions"]}
    result.update(
        all_signatures_match=True,
        positions=len(corpus["positions"]),
        source_games=len(games),
        by_model={
            entry["name"]: metrics([row for row in rows if row["model"] == index])
            for index, entry in enumerate(corpus["models"])
        },
        method="Equal-case geometric mean paired process CPU ratios; each model/root/repetition has equal weight.",
        limits="Warm fixed-depth searches on one host; loading/replay excluded; no clock, cold-start or strength claim.",
    )
    if len(games) >= 2:
        grouped = {
            game: [row for row in rows if corpus["positions"][row["position"]]["game"] == game]
            for game in sorted(games)
        }
        keys = list(grouped)
        rng = random.Random(87231)
        draws = sorted(
            metrics([row for game in rng.choices(keys, k=len(keys)) for row in grouped[game]])[
                "candidate_over_baseline"
            ]
            for _ in range(3000)
        )
        result["source_game_cluster_bootstrap_95"] = [draws[75], draws[2924]]
        result["interval_method"] = (
            "3000 deterministic draws of complete source-game clusters; few games limit inference."
        )
    return result


def verify(args):
    corpus_hash = workload.file_digest(args.corpus)
    corpus = json.loads(args.corpus.read_text())
    if not corpus["positions"] or not corpus["models"]:
        raise ValueError("verification requires nonempty positions and models")
    inputs = workload.audit_inputs(corpus, list(range(len(corpus["models"]))))
    args.out.mkdir(parents=True, exist_ok=False)
    binaries = {arm: str(getattr(args, arm).resolve(strict=True)) for arm in ("baseline", "candidate")}
    plan = dict(
        binaries=binaries,
        binary_sha256={arm: workload.file_digest(path) for arm, path in binaries.items()},
        corpus=corpus,
        corpus_sha256=corpus_hash,
        inputs=inputs,
        cpu=args.cpu,
        repeats=args.repeats,
        depth=args.depth,
        seed=args.seed,
        timer="Process user+system CPU milliseconds inside search(); model load/replay/legal validation excluded.",
    )
    workload.atomic_json(args.out / "plan.json", plan)
    env = {key: value for key, value in os.environ.items() if not key.startswith("TAIKYOKU_AB_")}
    env.pop("LLVM_PROFILE_FILE", None)
    rows = []
    rng = random.Random(args.seed)

    def request(workers, arm, query):
        reply = workers[arm].request(query)
        workload.validate_result(reply, query, corpus)
        for key in ("cpu_ms", "ms"):
            value = reply.get(key)
            if type(value) not in (float, int) or not math.isfinite(value) or value <= 0:
                raise ValueError(f"invalid {arm} {key}: {value}")
        return reply

    def pair(workers, query, order):
        values = {arm: request(workers, arm, query) for arm in order}
        for key in (
            "signature",
            "best",
            "score",
            "nodes",
            "q_nodes",
            "completed_depth",
            "hash",
            "pieces",
            "legal",
            "nnue",
        ):
            for arm in values:
                if key not in values[arm]:
                    raise ValueError(f"missing {arm} result field {key}: {query}")
            if values["baseline"][key] != values["candidate"][key]:
                raise ValueError(f"native/PGO mismatch in {key}: {query}")
        return dict(query, order=order, **values)

    with (args.out / "pairs.jsonl").open("w") as output, (args.out / "warmups.jsonl").open("w") as warm:
        for model, entry in enumerate(corpus["models"]):
            # Only this model is loaded in each worker, bounding peak model memory.
            with contextlib.ExitStack() as stack:
                workers = {}
                for arm in binaries:
                    error = stack.enter_context((args.out / f"model-{model}-{arm}.stderr").open("wb"))
                    workers[arm] = stack.enter_context(
                        workload.ProfileWorker(
                            ["taskset", "-c", str(args.cpu), binaries[arm], str(args.corpus.resolve())],
                            env=env,
                            stderr=error,
                            timeout_seconds=args.request_timeout_seconds,
                        )
                    )
                positions = list(range(len(corpus["positions"])))
                rng.shuffle(positions)
                starts = {position: bool(rng.getrandbits(1)) for position in positions}
                for position in positions:
                    row = pair(
                        workers, dict(position=position, model=model, depth=args.depth), ["baseline", "candidate"]
                    )
                    warm.write(json.dumps(row) + "\n")
                    warm.flush()
                for repetition in range(args.repeats):
                    rng.shuffle(positions)
                    for position in positions:
                        order = ["baseline", "candidate"]
                        if starts[position] ^ bool(repetition % 2):
                            order.reverse()
                        row = pair(workers, dict(position=position, model=model, depth=args.depth), order)
                        row["rep"] = repetition
                        rows.append(row)
                        output.write(json.dumps(row) + "\n")
                        output.flush()
            print(f"verified {entry['name']}: {len(positions) * args.repeats} exact pairs", flush=True)
    # Detect changed inputs/binaries before accepting the validation result.
    if workload.file_digest(args.corpus) != corpus_hash:
        raise ValueError("verification corpus changed during searches")
    if workload.audit_inputs(corpus, list(range(len(corpus["models"])))) != inputs:
        raise ValueError("verification inputs changed during searches")
    if any(workload.file_digest(path) != plan["binary_sha256"][arm] for arm, path in binaries.items()):
        raise ValueError("verification executable changed")
    summary = summarize(rows, corpus)
    workload.atomic_json(args.out / "summary.json", summary)
    print(json.dumps(summary, indent=2))
    if args.require_speedup and summary["candidate_over_baseline"] >= 1:
        raise ValueError("PGO did not reduce measured CPU; candidate was not selected")
    return summary


def interrupted(signum, _frame):
    # InterruptedError is retried by selectors; escape into worker cleanup.
    raise RuntimeError(f"verification interrupted by signal {signum}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("baseline", "candidate", "corpus", "out"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    parser.add_argument("--repeats", type=int, default=4)
    parser.add_argument("--depth", type=int, default=2)
    parser.add_argument("--seed", type=int, default=87231)
    parser.add_argument("--request-timeout-seconds", type=float, default=120)
    parser.add_argument("--require-speedup", action="store_true")
    args = parser.parse_args()
    if (
        args.repeats < 2
        or args.repeats % 2
        or args.depth < 1
        or not math.isfinite(args.request_timeout_seconds)
        or args.request_timeout_seconds <= 0
    ):
        parser.error("use positive depth/timeout and an even repeat count >=2")
    if args.cpu not in os.sched_getaffinity(0):
        parser.error("selected CPU is outside the allowed affinity mask")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    try:
        verify(args)
    except (OSError, ValueError, RuntimeError, InterruptedError, TimeoutError) as error:
        print(f"PGO verification failed: {error}; no engine was selected.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
