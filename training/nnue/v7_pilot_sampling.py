"""Bounded, training-only hard-situation sampling for the v7 sensitivity pilot."""

import math


def cell(row):
    ratio = row["material"] / max(1, row.get("material_total", 0))
    balance = -1 if ratio < -0.05 else 1 if ratio > 0.05 else 0
    return f"{row['phase']}/{balance}/{int(row.get('legal_promotion', False))}"


def partition(rows, ids, minimum=20):
    """Merge unsupported promotion cells, then material cells, within each phase."""
    keys = {i: cell(rows[i]) for i in ids}
    for level in (2, 1):
        families = {}
        for i, key in keys.items():
            prefix = "/".join(key.split("/")[:level])
            support = families.setdefault(prefix, {}).setdefault(key, set())
            if rows[i].get("raw_confidence", 0) >= 0.5:
                support.add(rows[i]["group"])
        for prefix, children in families.items():
            if any(len(groups) < minimum for groups in children.values()):
                keys = {i: prefix if k in children else k for i, k in keys.items()}
    support = {}
    for i, key in keys.items():
        if rows[i].get("raw_confidence", 0) >= 0.5:
            support.setdefault(key, set()).add(rows[i]["group"])
    return {i: key if len(support.get(key, ())) >= minimum else "unsupported" for i, key in keys.items()}


def initialize(rows, ids, measure, limit=2048):
    keys = partition(rows, ids)
    groups = {}
    for i in ids:
        if keys[i] != "unsupported" and rows[i].get("raw_confidence", 0) >= 0.5:
            groups.setdefault(rows[i]["group"], []).append(i)
    # Round-robin selection keeps large games from occupying the fixed audit.
    audit = []
    for offset in range(max((len(v) for v in groups.values()), default=0)):
        audit.extend(v[offset] for _, v in sorted(groups.items()) if offset < len(v))
        if len(audit) >= limit:
            audit = audit[:limit]
            break
    cells = {}
    for i in audit:
        cells.setdefault(keys[i], []).append(i)
    # The capped audit itself must retain the independent-group support.
    cells = {k: v for k, v in cells.items() if len({rows[i]["group"] for i in v}) >= 20}
    keys = {str(i): k if k in cells else "unsupported" for i, k in keys.items()}
    initial = {k: measure(v) for k, v in cells.items()}
    return dict(
        keys=keys, audit=cells, initial=initial, ema=dict(initial), multipliers=dict.fromkeys(cells, 1.0), updates=0
    )


def update(state, rows, ids, measure):
    for key, audit in state["audit"].items():
        state["ema"][key] = 0.8 * state["ema"][key] + 0.2 * measure(audit)
    ratios = {k: v / max(state["initial"][k], 0.002) for k, v in state["ema"].items()}
    mass = dict.fromkeys(ratios, 0.0)
    for i in ids:
        key = state["keys"][str(i)]
        if key in mass:
            mass[key] += rows[i]["sampling_weight"]
    if ratios and max(ratios.values()) > 0:
        # Positive floor makes the mass-preserving root attainable even at zero error.
        ratios = {k: max(v, 1e-8) for k, v in ratios.items()}
        lower, upper = 0.0, 2 / min(ratios.values())
        for _ in range(80):
            middle = (lower + upper) / 2
            value = sum(mass[k] * min(2, max(0.5, middle * ratios[k])) for k in mass)
            if value < sum(mass.values()):
                lower = middle
            else:
                upper = middle
        state["multipliers"] = {k: min(2, max(0.5, (lower + upper) / 2 * v)) for k, v in ratios.items()}
    else:
        state["multipliers"] = dict.fromkeys(ratios, 1.0)
    state["updates"] += 1
    weights = sampling_weights(state, rows, ids)
    if not math.isclose(sum(weights), sum(rows[i]["sampling_weight"] for i in ids), abs_tol=1e-8):
        raise ValueError("adaptive sampler changed total mass")
    return dict(
        multipliers=dict(state["multipliers"]),
        supported_mass=sum(mass.values()),
        total_variation=sum(abs(w - rows[i]["sampling_weight"]) for i, w in zip(ids, weights, strict=True)) / 2,
        updates=state["updates"],
    )


def sampling_weights(state, rows, ids):
    return [rows[i]["sampling_weight"] * state["multipliers"].get(state["keys"][str(i)], 1.0) for i in ids]
