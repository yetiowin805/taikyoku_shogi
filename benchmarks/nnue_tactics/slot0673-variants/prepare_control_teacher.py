#!/usr/bin/env python3
"""Freeze distinct modal control moves before obtaining teacher opinions."""
from collections import Counter
import datetime as dt
import json
from pathlib import Path

BASE = Path(__file__).resolve().parent
CORPUS = json.loads((BASE / "corpus.json").read_text())
variants = ("baseline", "tempo_50", "q_tempo_50")
selected = []
for pos in CORPUS["positions"]:
    if pos["group"] != "heldout":
        continue
    modes = {}
    for variant in variants:
        trials = [json.loads((BASE / "runs/timed-hard" / f"{pos['id']}-{variant}-r{repeat}.json").read_text())
                  for repeat in (1, 2, 3)]
        counts = Counter(t["best_move"] for t in trials)
        chosen = sorted(counts, key=lambda route: (-counts[route], route or ""))[0]
        modes[variant] = {"route": chosen, "votes": counts[chosen], "all_routes": dict(counts)}
    if len({v["route"] for v in modes.values()}) < 2:
        continue
    routes = {}
    for variant in variants:
        route = modes[variant]["route"]
        if route is not None:
            routes.setdefault(route, []).append(variant)
    trial_variants = {
        "_".join(names): {"env": {"TACTICAL_ROOT": route}, "purpose": f"SEEDS2 opinion of modal move selected by {', '.join(names)}"}
        for route, names in routes.items()
    }
    selected.append({"position": pos["id"], "ply": pos["ply"], "side_to_move": pos["side_to_move"],
                     "modes": modes, "variants": trial_variants})
out = BASE / "control-teacher-selection.json"
if out.exists():
    raise SystemExit("Refusing to overwrite frozen teacher-selection manifest")
out.write_text(json.dumps({
    "frozen_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
    "method": "All ten preselected controls; compare modal routes over three 3-second hard-budget trials for baseline, global half-tempo, and q-only half-tempo. Break modal ties lexicographically. Retain all distinct routes whenever those variants disagree. Freeze before teacher searches.",
    "teacher_policy": "SEEDS2 separate forced-root depth 3 and 4 searches, 5-second limit. Teacher opinions are not optimality labels.",
    "selected": selected,
}, indent=2) + "\n")
for entry in selected:
    (BASE / f"teacher-control-{entry['position']}.json").write_text(json.dumps(entry["variants"], indent=2) + "\n")
print(json.dumps({"positions": len(selected), "distinct_routes": sum(len(e["variants"]) for e in selected), "selection": str(out)}))
