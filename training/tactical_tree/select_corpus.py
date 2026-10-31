"""Freeze six existing diagnostics and six new episodes using saved scores only."""

import argparse
import hashlib
import json
from pathlib import Path

from run import atomic, sha


def select(run, previous, seed):
    old = json.loads(previous.read_text())
    excluded = {c["slot"] for c in old["cases"]} | {672, 769, 867}
    cases = [dict(c, cohort="diagnostic") for c in old["cases"][:6]]
    state = json.loads((run / "state.json").read_text())
    pools = {
        name: [] for name in ("early_swing", "middle_swing", "long_history", "mate_onset", "nnue_crossing", "quiet")
    }
    for slot in state["slots"]:
        if slot["status"] != "done" or slot["id"] < 1300 or slot["id"] in excluded:
            continue
        path = Path(slot["game_path"])
        game = json.loads(path.read_text())
        moves = game["moves"]
        for i in range(4, len(moves) - 4):
            a, b = moves[i - 2], moves[i]
            x, y = a.get("eval"), b.get("eval")
            if type(x) is not int or type(y) is not int or a["color"] != b["color"]:
                continue
            finite = abs(x) < 900000 and abs(y) < 900000
            swing = finite and abs(x - y) >= 1000
            flags = dict(
                early_swing=swing and i < 200,
                middle_swing=swing and 200 <= i < 800,
                long_history=swing and i >= 1000,
                mate_onset=abs(x) < 900000 <= abs(y),
                nnue_crossing=swing and x * y < 0 and all(slot[k].startswith("NNUE_") for k in ("model_a", "model_b")),
                quiet=finite and abs(x - y) < 100 and abs(y) < 2000 and 100 <= i < len(moves) - 50,
            )
            for name, eligible in flags.items():
                if eligible:
                    rank = hashlib.sha256(f"{seed}:{name}:{slot['id']}:{i}".encode()).hexdigest()
                    pools[name].append((rank, slot["id"], i, path, moves[i - 4]["color"], len(moves), x, y))
    used = set(excluded)
    for name, pool in pools.items():
        available = [c for c in pool if c[1] not in used]
        if not available:
            raise ValueError("no fresh game for category " + name)
        _, slot, i, path, color, count, x, y = min(available)
        used.add(slot)
        cases.append(
            dict(
                id=f"s{slot:04}-p{i + 1:04}",
                slot=slot,
                anchor_ply=i + 1,
                start_ply=i - 3,
                end_ply=i + 5,
                start_color=color,
                game=str(path),
                game_filename=path.name,
                sha256=sha(path),
                plies=count,
                selection_reason=name,
                cohort="fresh",
                saved_scores_black=[x, y],
            )
        )
    for case in cases:
        case["audit_plies"] = [case["anchor_ply"] + offset for offset in (-2, 0, 2)]
    return dict(
        schema=2,
        seed=seed,
        selection="Saved-score-only hashed sampling before comparison results; six diagnostics and six fresh games. Fixed audit locations.",
        cases=cases,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", type=Path, required=True)
    parser.add_argument("--previous", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=20261018)
    args = parser.parse_args()
    if args.out.exists():
        parser.error("corpus already frozen; choose a new path")
    atomic(args.out, select(args.run, args.previous, args.seed))
