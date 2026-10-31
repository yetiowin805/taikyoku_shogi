"""Audit adjacent-score disagreement in an immutable local game collection."""

import argparse
import hashlib
import json
import subprocess
import sys
from collections import defaultdict
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "deploy"))
    import training_labels as labels

    groups = defaultdict(list)
    selected_groups = defaultdict(list)
    eligible_groups = defaultdict(list)
    manifest = []
    per_game = []
    for path in sorted(args.directory.glob("*.json")):
        data = path.read_bytes()
        game = json.loads(data)
        names = [part for side in ("black", "white") for part in Path(game[side]["model"]).parts]
        name = next((n for n in names if "v4.5" in n), "other")
        manifest.append(dict(file=path.name, sha256=hashlib.sha256(data).hexdigest()))
        values = []
        moves = game["moves"]
        excluded = labels.mate_exclusions(game)
        for i in range(len(moves)):
            d = labels.disagreement(game, i)
            if d is None:
                continue
            groups[name].append(d)
            groups["all"].append(d)
            values.append(d)
            if i not in excluded:
                eligible_groups[name].append(d)
                eligible_groups["all"].append(d)
        selections = [r for r in labels.candidates(game) if r.get("ordinary_role")]
        for r in selections:
            if r["disagreement"] is not None:
                selected_groups[r["ordinary_role"]].append(r["disagreement"])
        per_game.append(dict(file=path.name, family=name, selections=selections, **describe(values)))
    print(
        json.dumps(
            dict(
                scale=labels.PROBABILITY_SCALE,
                revision=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
                sampler_sha256=hashlib.sha256(Path(labels.__file__).read_bytes()).hexdigest(),
                manifest=manifest,
                groups={k: describe(v) for k, v in groups.items()},
                eligible_groups={k: describe(v) for k, v in eligible_groups.items()},
                selected_groups={k: describe(v) for k, v in selected_groups.items()},
                games=per_game,
            ),
            indent=2,
        )
    )


def describe(values):
    values = sorted(values)
    if not values:
        return dict(n=0)
    return dict(
        n=len(values),
        mean=sum(values) / len(values),
        quantiles={str(q): values[int((len(values) - 1) * q)] for q in (0.25, 0.5, 0.75, 0.9, 0.95)},
        fraction_above_30pp=sum(d >= 0.30 for d in values) / len(values),
    )


if __name__ == "__main__":
    main()
