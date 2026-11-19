"""Aggregate paired observations and bootstrap entire source games."""

import collections
import gzip
import json
import math
import random
import statistics

from replay import HERE


def load(path):
    with gzip.open(path, "rt") as f:
        return [json.loads(line) for line in f]


def effect(pairs):
    # Within each game give equal weight to its two balanced passes.
    logs = [statistics.mean(math.log(y / x) for x, y in values) for values in pairs.values()]
    rng = random.Random(20261119)
    boot = sorted(math.exp(statistics.mean(rng.choices(logs, k=len(logs)))) for _ in range(5000))
    values = [p for ps in pairs.values() for p in ps]
    return dict(
        ratio=math.exp(statistics.mean(logs)),
        interval=[boot[125], boot[4874]],
        baseline=statistics.geometric_mean(x for x, _ in values),
        candidate=statistics.geometric_mean(y for _, y in values),
        summed_ratio=sum(y for _, y in values) / sum(x for x, _ in values),
        games=len(logs),
    )


def compare(rows, key, reference, candidate, measure):
    cells = collections.defaultdict(dict)
    for r in rows:
        group, case = key(r)
        cells[group, case, r["rep"]][r["arm"]] = r
    pairs = collections.defaultdict(list)
    for (group, _, _), cell in cells.items():
        pairs[group].append((measure(cell[reference]), measure(cell[candidate])))
    return effect(pairs)


def main():
    build, *rows = load(HERE / "replay.jsonl.gz")
    assert len(rows) == 144
    games = load(HERE / "game-worker.jsonl.gz")
    assert len(games) == 24
    summaries = {}
    for pair in ("384", "512", "1536"):
        sub = [r for r in rows if r["pair"] == pair]
        metrics = {
            "first_two": lambda r: sum(x["cpu_ms"] for x in r["replies"][:2]) / 2,
            "return": lambda r: sum(x["cpu_ms"] for x in r["replies"][2:]) / 4,
            "all_search": lambda r: sum(x["cpu_ms"] for x in r["replies"]) / 6,
            "process_cpu": lambda r: r["cpu"],
            "process_wall": lambda r: r["wall"],
            "pss_mib": lambda r: r["pss_kib"] / 1024,
        }
        for model in {x["model"] for r in sub for x in r["replies"]}:

            def mean_cpu(r, mid=model):
                return statistics.mean(x["cpu_ms"] for x in r["replies"][2:] if x["model"] == mid)

            metrics["return_model_" + str(model)] = mean_cpu
        summaries[pair] = {}
        for arm, ref in [("owned", "stock"), ("placebo", "stock"), ("owned", "bounded")]:
            summaries[pair][arm + "/" + ref] = {
                k: compare(sub, lambda r: (r["slot"], r["pair"]), ref, arm, f) for k, f in metrics.items()
            }
    worker = {}
    for name, observed in [("short", games), ("long", load(HERE / "long-worker.jsonl.gz"))]:
        assert len(observed) == 24
        worker[name] = {
            k: compare(observed, lambda r: (r["source_game"], r["position"]), "stock", "owned", lambda r, key=k: r[key])
            for k in ("cpu", "wall", "play_ms")
        }
        for r in observed:
            assert 0 < r["plies"] <= (12 if name == "short" else 64)
        worker[name]["plies_per_arm"] = sum(r["plies"] for r in observed if r["arm"] == "owned")
    interleaved = load(HERE / "interleaved.jsonl.gz")
    assert len(interleaved) == 72
    checks = {}
    for ref in ("bounded", "control"):
        pairs = collections.defaultdict(list)
        for row in interleaved:
            if row["step"] >= 2:
                m = row["measurements"]
                pairs[row["slot"]].append((m[ref]["cpu_ms"], m["owned"]["cpu_ms"]))
        checks["owned/" + ref] = effect(pairs)
    print(json.dumps(dict(builds=build, pairs=summaries, game_worker=worker, interleaved=checks), indent=2))


if __name__ == "__main__":
    main()
