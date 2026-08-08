#!/usr/bin/env python3
"""Correlate scale-sample multipliers with Swiss / tourney scores.

Example (on the VPS after a Swiss finishes):

  python3 deploy/scale_sample_corr.py \\
    --samples models/scale-sample/samples.json \\
    --state data/raw/tourney/swiss-20260806T164515Z/state.json

By default only random R* entrants are used (seed / all_m10 / all_p10 are
controls that move every knob together and would dominate the correlation).
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from collections import defaultdict
from pathlib import Path
from typing import Dict, Iterable, List, Optional, Sequence, Tuple


def pearson(xs: Sequence[float], ys: Sequence[float]) -> Optional[float]:
    n = len(xs)
    if n < 3 or n != len(ys):
        return None
    mx = sum(xs) / n
    my = sum(ys) / n
    num = 0.0
    dx2 = 0.0
    dy2 = 0.0
    for x, y in zip(xs, ys):
        dx = x - mx
        dy = y - my
        num += dx * dy
        dx2 += dx * dx
        dy2 += dy * dy
    if dx2 <= 1e-18 or dy2 <= 1e-18:
        return None  # no variance in x or y
    return num / math.sqrt(dx2 * dy2)


def spearman(xs: Sequence[float], ys: Sequence[float]) -> Optional[float]:
    """Pearson of average ranks (ties get mid-rank)."""
    n = len(xs)
    if n < 3 or n != len(ys):
        return None

    def ranks(vals: Sequence[float]) -> List[float]:
        order = sorted(range(n), key=lambda i: vals[i])
        out = [0.0] * n
        i = 0
        while i < n:
            j = i
            while j + 1 < n and vals[order[j + 1]] == vals[order[i]]:
                j += 1
            # ranks are 1-based; mid-rank for ties
            mid = (i + j) / 2.0 + 1.0
            for k in range(i, j + 1):
                out[order[k]] = mid
            i = j + 1
        return out

    return pearson(ranks(xs), ranks(ys))


def match_scores(state: dict) -> Dict[str, float]:
    scores: Dict[str, float] = defaultdict(float)
    for e in state.get("entrants", []):
        scores[e["id"]] = 0.0
    for slot in state.get("slots", []):
        if slot.get("status") != "done":
            continue
        sa = slot.get("score_a")
        if sa is None:
            continue
        a = slot["model_a"]
        b = slot["model_b"]
        scores[a] += float(sa)
        scores[b] += 1.0 - float(sa)
    return dict(scores)


def load_elo(path: Path) -> Dict[str, float]:
    raw = json.loads(path.read_text())
    return {k: float(v) for k, v in raw.items()}


def is_random_sample(sid: str) -> bool:
    return sid.startswith("R") and sid[1:].isdigit()


def mean(xs: Sequence[float]) -> float:
    return sum(xs) / len(xs) if xs else float("nan")


def fmt_r(r: Optional[float]) -> str:
    if r is None:
        return "  n/a "
    return f"{r:+.3f}"


def analyze(
    samples_path: Path,
    state_path: Path,
    elo_path: Optional[Path],
    include_controls: bool,
    metric: str,
) -> int:
    samples = json.loads(samples_path.read_text())
    state = json.loads(state_path.read_text())
    names: List[str] = list(samples["param_names"])
    by_id = {s["id"]: s for s in samples["samples"]}

    scores = match_scores(state)
    elo = load_elo(elo_path) if elo_path and elo_path.exists() else {
        e["id"]: float(state.get("elo", {}).get(e["id"], 1500.0))
        for e in state.get("entrants", [])
    }
    # state.json usually embeds elo
    if "elo" in state and isinstance(state["elo"], dict):
        for k, v in state["elo"].items():
            elo.setdefault(k, float(v))

    ids = []
    for sid in sorted(by_id):
        if sid not in scores and sid not in elo:
            continue
        if not include_controls and not is_random_sample(sid):
            continue
        ids.append(sid)

    if len(ids) < 5:
        print(
            f"Need more entrants with scores (got {len(ids)}). "
            "Try --include-controls or check paths.",
            file=sys.stderr,
        )
        return 1

    y_score = [scores.get(i, 0.0) for i in ids]
    y_elo = [elo.get(i, 1500.0) for i in ids]
    y = y_score if metric == "score" else y_elo
    y_label = "match_score" if metric == "score" else "elo"

    print(f"samples: {samples_path}")
    print(f"state:   {state_path}")
    print(f"entrants used: {len(ids)}  metric={y_label}  controls={'on' if include_controls else 'off (R* only)'}")
    print(f"score range: {min(y_score):.1f} .. {max(y_score):.1f}   elo range: {min(y_elo):.1f} .. {max(y_elo):.1f}")
    print()

    # Per-param correlations
    rows = []
    for j, name in enumerate(names):
        xs = [float(by_id[i]["multipliers"][j]) for i in ids]
        r_p = pearson(xs, y)
        r_s = spearman(xs, y)
        # mean target by multiplier level
        by_lvl: Dict[float, List[float]] = defaultdict(list)
        for x, yi in zip(xs, y):
            by_lvl[round(x, 3)].append(yi)
        levels = sorted(by_lvl)
        level_means = {lv: mean(by_lvl[lv]) for lv in levels}
        # effect: mean(1.1) - mean(0.9) when both present
        effect = None
        if 0.9 in level_means and 1.1 in level_means:
            effect = level_means[1.1] - level_means[0.9]
        rows.append((name, r_p, r_s, effect, level_means, xs))

    # Sort by |pearson|, falling back to |spearman|, then |effect|
    def sort_key(row):
        name, r_p, r_s, effect, *_ = row
        key_r = abs(r_p) if r_p is not None else (abs(r_s) if r_s is not None else -1.0)
        key_e = abs(effect) if effect is not None else -1.0
        return (key_r, key_e)

    rows.sort(key=sort_key, reverse=True)

    print(f"{'param':20s}  {'pearson':>8s}  {'spearman':>8s}  {'Δ(1.1−0.9)':>10s}  mean@0.9  mean@1.0  mean@1.1  n_var")
    print("-" * 100)
    for name, r_p, r_s, effect, level_means, xs in rows:
        n_unique = len({round(x, 3) for x in xs})
        def m(lv: float) -> str:
            return f"{level_means[lv]:7.2f}" if lv in level_means else "    n/a"
        eff_s = f"{effect:+10.2f}" if effect is not None else "       n/a"
        print(
            f"{name:20s}  {fmt_r(r_p):>8s}  {fmt_r(r_s):>8s}  {eff_s}  "
            f"{m(0.9)}  {m(1.0)}  {m(1.1)}  {n_unique}"
        )

    print()
    print("Notes:")
    print("- n≈30 and multipliers are only {0.9,1.0,1.1}; treat |r|≲0.3 as noise.")
    print("- Δ(1.1−0.9) is usually the most readable effect size on this grid.")
    print("- Positive r / Δ means higher multiplier ↔ higher", y_label + ".")
    if not include_controls:
        print("- Controls (seed, all_m10, all_p10) excluded; pass --include-controls to add them.")

    # Top / bottom agents quick dump
    ranked = sorted(zip(ids, y), key=lambda t: t[1], reverse=True)
    print()
    print(f"Top 3 by {y_label}:")
    for sid, yi in ranked[:3]:
        s = by_id[sid]
        changed = [
            (n, m)
            for n, m in zip(names, s["multipliers"])
            if abs(float(m) - 1.0) > 1e-6
        ]
        print(f"  {sid:6s}  {y_label}={yi:.1f}  changed={changed}")
    print(f"Bottom 3 by {y_label}:")
    for sid, yi in ranked[-3:]:
        s = by_id[sid]
        changed = [
            (n, m)
            for n, m in zip(names, s["multipliers"])
            if abs(float(m) - 1.0) > 1e-6
        ]
        print(f"  {sid:6s}  {y_label}={yi:.1f}  changed={changed}")

    return 0


def main(argv: Optional[Sequence[str]] = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument(
        "--samples",
        type=Path,
        default=Path("models/scale-sample/samples.json"),
        help="scale-sample samples.json",
    )
    p.add_argument(
        "--state",
        type=Path,
        required=True,
        help="tourney state.json (has slots + scores)",
    )
    p.add_argument(
        "--elo",
        type=Path,
        default=None,
        help="optional elo.json (defaults to elo embedded in state)",
    )
    p.add_argument(
        "--metric",
        choices=("score", "elo"),
        default="score",
        help="correlate against match points (default) or elo",
    )
    p.add_argument(
        "--include-controls",
        action="store_true",
        help="include seed / all_m10 / all_p10 (usually distorts correlations)",
    )
    args = p.parse_args(argv)

    if not args.samples.is_file():
        print(f"missing samples: {args.samples}", file=sys.stderr)
        return 2
    if not args.state.is_file():
        print(f"missing state: {args.state}", file=sys.stderr)
        return 2
    elo = args.elo
    if elo is None:
        cand = args.state.parent / "elo.json"
        elo = cand if cand.is_file() else None
    return analyze(args.samples, args.state, elo, args.include_controls, args.metric)


if __name__ == "__main__":
    sys.exit(main())
