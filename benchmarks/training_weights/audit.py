"""Read-only audit of cached parent errors; no neural-network inference."""

import argparse
import hashlib
import json
import math
from collections import Counter, defaultdict
from pathlib import Path


def curve(error, limit, smooth):
    t = min(error / limit, 1)
    return 1 + (t * t * (3 - 2 * t) if smooth else t)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dataset", type=Path)
    args = parser.parse_args()
    cells = defaultdict(list)
    errors = []
    bands = Counter()
    strata = defaultdict(list)
    digest = hashlib.sha256()
    for line in (args.dataset / "samples.jsonl").open("rb"):
        digest.update(line)
        row = json.loads(line)
        if row["split"] != "train":
            continue
        e = row["parent_probability_error"]
        old = row["difficulty_multiplier"]
        assert math.isfinite(e) and 0 <= e <= 1
        assert old == (1 if e < 0.10 else 1.5 if e <= 0.25 else 2)
        w = row["sampling_weight"]
        cells[row["stratum"], row["group"], row["game"], row["episode"]].append((e, old, w))
        errors.append(e)
        bands[str(old)] += 1
        strata[row["stratum"]].append(e)

    def describe(es):
        es = sorted(es)
        return dict(
            n=len(es),
            mean=sum(es) / len(es),
            quantiles={str(q): es[int((len(es) - 1) * q)] for q in (0.1, 0.25, 0.5, 0.75, 0.9, 0.95, 0.99)},
        )

    variants = {}
    total = sum(w for rows in cells.values() for _, _, w in rows)
    for name, limit, smooth in [
        ("smoothstep_035", 0.35, True),
        ("smoothstep_030", 0.30, True),
        ("linear_035", 0.35, False),
    ]:
        tv = 0
        ratios = []
        mass = Counter()
        before = Counter()
        max_cell_error = 0
        for rows in cells.values():
            cell_total = sum(w for _, _, w in rows)
            provisional = [w / old * curve(e, limit, smooth) for e, old, w in rows]
            scale = cell_total / sum(provisional)
            new = [w * scale for w in provisional]
            max_cell_error = max(max_cell_error, abs(sum(new) - cell_total))
            for (_e, old, w), nw in zip(rows, new, strict=True):
                tv += abs(nw - w)
                ratios.append(nw / w)
                mass[str(old)] += nw
                before[str(old)] += w
        variants[name] = dict(
            total_variation=tv / (2 * total),
            effective_weight_ratios=describe(ratios),
            old_band_mass={k: v / total for k, v in before.items()},
            new_band_mass={k: v / total for k, v in mass.items()},
            max_cell_mass_error=max_cell_error,
        )
    identity = json.loads((args.dataset / "dataset.json").read_text())
    assert identity["samples_sha256"] == digest.hexdigest()
    print(
        json.dumps(
            dict(
                dataset=str(args.dataset),
                samples_sha256=digest.hexdigest(),
                weighting=identity.get("weighting"),
                errors=describe(errors),
                old_multiplier_counts=bands,
                strata={k: describe(v) for k, v in strata.items()},
                cells=len(cells),
                variants=variants,
            ),
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
