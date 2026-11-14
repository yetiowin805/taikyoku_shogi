"""Full v7 sampling: all eligible families, capped games, untouched inherited splits."""

import datetime
import math
from collections import defaultdict

import v6_data as data


def completion_time(row):
    value = row.get("timestamp")
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, str):
        parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            parsed = parsed.replace(tzinfo=datetime.timezone.utc)
        return parsed.timestamp()
    raise ValueError("fresh v7 groups need recorded completion timestamps for the recent test split")


def select_games(records, config):
    groups = defaultdict(list)
    for row in records.values():
        if not row["quarantine"]:
            groups[row["group"]].append(row)
    fresh = [key for key, rows in groups.items() if not any(r["inherited"] for r in rows)]
    if len(fresh) < config.get("minimum_fresh_groups", 100):
        raise ValueError("insufficient fresh related groups for independent v7 validation/test")
    count = max(1, math.ceil(len(fresh) * 0.10))
    recent = sorted(fresh, key=lambda key: (max(completion_time(r) for r in groups[key]), key))[-count:]
    test = set(recent)
    validation = set(
        sorted(
            (key for key in fresh if key not in test), key=lambda key: data.hashed([config["seed"], "validation", key])
        )[:count]
    )
    for key in fresh:
        split = "test" if key in test else "validation" if key in validation else "train"
        for row in groups[key]:
            row["split"] = split
    roots_per_game = config["representative"] + config["challenge"]
    limit = config.get("roots_per_group", 40) // roots_per_game
    if limit < 1:
        raise ValueError("group cap cannot accommodate one game's root allocation")
    selected = {}
    for _key, rows in sorted(groups.items()):
        candidates = sorted(rows, key=lambda row: data.hashed([config["seed"], "game-coverage", row["id"]]))[:limit]
        for row in candidates:
            selected[row["id"]] = row
    return selected


def audit_selection(rows, fraction, seed, priority):
    selected = []
    for split in ("train", "validation", "test"):
        subset = [row for row in rows if row["split"] == split]
        count = math.ceil(len(subset) * fraction)
        if split == "train":
            selected.extend(data.mixed_selection(subset, count, [seed, split], priority))
        else:
            selected.extend(sorted(subset, key=lambda row: data.hashed([seed, split, row["id"]]))[:count])
    return selected
