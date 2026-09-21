#!/usr/bin/env python3
"""Summarize paired 3-second NPS ratios; equal weight to every position."""
import json
import math
from pathlib import Path
import statistics
import sys

root = Path(sys.argv[1])
rows = [json.loads(s) for s in (root / "timed.jsonl").read_text().splitlines()]
verified = [json.loads(s) for s in (root / "verify.jsonl").read_text().splitlines()]
manifest = json.loads((root / "timed-manifest.json").read_text())
variants = list(manifest["variants"])
assert len(rows) == len(manifest["order"])
by_key = {(r["width"], r["position"], r["rep"], r["variant"]): r for r in rows}
assert len(by_key) == len(rows)


def geo(xs):
    return math.exp(statistics.mean(math.log(x) for x in xs))


def nps(row):
    return row["result"]["nodes"] / (row["result"]["elapsed_ns"] / 1e9)


def comparison(width, numerator, denominator):
    selected = [r for r in rows if r["width"] == width and r["variant"] == numerator]
    pairs = [(r, by_key[width, r["position"], r["rep"], denominator]) for r in selected]
    ratios = [nps(a) / nps(b) for a, b in pairs]
    per_position = {
        pos: geo(nps(a) / nps(b) for a, b in pairs if a["position"] == pos)
        for pos in sorted({a["position"] for a, _ in pairs})
    }
    pooled = lambda group: sum(r["result"]["nodes"] for r in group) / sum(r["result"]["elapsed_ns"] / 1e9 for r in group)
    fixed_pairs = [(r, next(b for b in verified if b["width"] == width and b["position"] == r["position"] and b["variant"] == denominator))
                   for r in verified if r["width"] == width and r["variant"] == numerator]
    return {
        "numerator": numerator, "denominator": denominator, "pairs": len(pairs),
        "equal_position_nps_ratio": geo(per_position.values()),
        "pooled_nps_ratio": pooled([a for a, _ in pairs]) / pooled([b for _, b in pairs]),
        "per_position_ratio": per_position,
        "per_repetition_ratio": [geo(nps(a) / nps(b) for a, b in pairs if a["rep"] == rep) for rep in sorted({a["rep"] for a, _ in pairs})],
        "single_fixed_depth_speed_ratio": geo(b["result"]["elapsed_ns"] / a["result"]["elapsed_ns"] for a, b in fixed_pairs),
        "fixed_depth_per_position_ratio": {a["position"]: b["result"]["elapsed_ns"] / a["result"]["elapsed_ns"] for a, b in fixed_pairs},
        "median_peak_rss_ratio": statistics.median(a["peak_rss_kib"] / b["peak_rss_kib"] for a, b in pairs),
        "median_peak_rss_delta_mib": statistics.median((a["peak_rss_kib"] - b["peak_rss_kib"]) / 1024 for a, b in pairs),
        "timed_best_move_changes": sum(a["result"]["best"] != b["result"]["best"] for a, b in pairs),
        "depth_pairs": [{"position": a["position"], "rep": a["rep"], "numerator": a["result"]["depth"], "denominator": b["result"]["depth"]} for a, b in pairs],
    }


# Recheck raw parity before publishing; timing success is not correctness evidence.
for row in verified:
    base = next(r for r in verified if r["width"] == row["width"] and r["position"] == row["position"] and r["variant"] == "main")
    assert not row["result"]["aborted"] and row["result"]["depth"] == 2
    for key in ["score", "static_score", "nodes", "qnodes", "best", "root_lines"]:
        assert row["result"][key] == base["result"][key], (row["variant"], key)

summary = {
    "main_revision": manifest["main_revision"], "source_revision": manifest["revision"],
    "timed_searches": len(rows), "verified_searches": len(verified),
    "method": "Geometric mean of paired NPS ratios, equal weight per position; two balanced repetitions. No confidence claim from four positions.",
    "widths": {str(w): {
        "versus_main": {v: comparison(w, v, "main") for v in variants if v != "main"},
        "marginal_contribution": {v: comparison(w, "full", v) for v in variants if v.startswith("no-")},
    } for w in [512, 2048]},
    "max_search_overshoot_ms": max(r["result"]["elapsed_ns"] / 1e6 - 3000 for r in rows),
    "all_legal": all(r["result"]["legal"] for r in rows + verified),
}
(root / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
for w, results in summary["widths"].items():
    print("Width", w)
    for label, groups in results.items():
        for variant, result in groups.items():
            print(label, variant, f'{(result["equal_position_nps_ratio"] - 1) * 100:+.2f}%', "reps", [round((x - 1) * 100, 2) for x in result["per_repetition_ratio"]])
