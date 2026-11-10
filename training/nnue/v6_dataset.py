"""Freeze completed v6 evidence into one leak-checked corpus for all eight runs."""

import array
import json
import math
import os
import sqlite3
import subprocess
from collections import Counter, defaultdict
from pathlib import Path

import v6_data as data
from v6_collect import STOP, Oracle, atomic

SCALE = 2822.9057434005167


def completed(db):
    stages = [data.get_meta(db, key) for key in ("initialized", "audit_planned", "deep_planned")]
    count = db.execute("SELECT count(*) FROM jobs").fetchone()[0]
    unfinished = db.execute("SELECT count(*) FROM jobs WHERE status!='completed'").fetchone()[0]
    return bool(count and all(x is not None for x in stages) and not unfinished)


def connect(path):
    db = sqlite3.connect("file:" + str(Path(path).resolve()) + "?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    return db


def temperature(rows):
    eligible = [
        r for r in rows if r["split"] == "train" and r["outcome"] is not None and r["stratum"] == "representative"
    ]
    counts = Counter(r["game"] for r in eligible)
    if len(counts) < 100:
        return SCALE, dict(reason="insufficient verified training games", games=len(counts))
    candidates = [SCALE * 2 ** (i / 8) for i in range(-16, 17)]
    losses = [
        sum(
            (sum(data.probability(s, t) for s in r["scores"]) / len(r["scores"]) - r["outcome"]) ** 2
            / counts[r["game"]]
            for r in eligible
        )
        / len(counts)
        for t in candidates
    ]
    best = min(range(len(losses)), key=losses.__getitem__)
    # Never silently extrapolate from a boundary optimum.
    chosen = candidates[best] if 0 < best < len(candidates) - 1 else SCALE
    return chosen, dict(
        games=len(counts),
        candidates=candidates,
        brier=losses,
        chosen=chosen,
        boundary_fallback=best in (0, len(candidates) - 1),
    )


def sampling_weights(rows):
    """One hierarchy: stratum -> group -> game -> phase -> distinct feature row."""
    proportions = dict(representative=0.75, challenge=0.20, alternative=0.05)
    for split in ("train", "validation", "test"):
        pool = [r for r in rows if r["split"] == split]
        tree = defaultdict(lambda: defaultdict(lambda: defaultdict(lambda: defaultdict(list))))
        for r in pool:
            tree[r["stratum"]][r["group"]][r["game"]][r["phase"]].append(r)
        normalizer = sum(proportions[s] for s in tree)
        for stratum, groups in tree.items():
            for games in groups.values():
                for phases in games.values():
                    for members in phases.values():
                        weight = (
                            proportions[stratum] / normalizer / len(groups) / len(games) / len(phases) / len(members)
                        )
                        for r in members:
                            r["sampling_weight"] = weight
        for stratum in tree:
            members = [r for r in pool if r["stratum"] == stratum]
            mean = sum(r["confidence"] * r["sampling_weight"] for r in members) / sum(
                r["sampling_weight"] for r in members
            )
            for r in members:
                r["confidence"] /= mean


def select_rows(db):
    games = {r["id"]: json.loads(r["payload"]) for r in db.execute("SELECT * FROM games")}
    labels = defaultdict(list)
    for job in db.execute(
        "SELECT position_id,teacher,tier,result FROM jobs WHERE status='completed' AND tier NOT IN ('outcome','alternatives')"
    ):
        result = json.loads(job["result"])
        label = result.get("label")
        if not label or label.get("mate_claim") or label.get("bound") != "completed_root_search":
            continue  # Unverified search mate is not ordinary regression evidence.
        if label.get("score_perspective") != "black-absolute" or not math.isfinite(label["score_black"]):
            raise ValueError("invalid label orientation or score")
        teacher = result.get("teacher", {})
        family = teacher.get("family")
        if job["teacher"] not in ("primary", "challenger") or not family:
            continue
        labels[job["position_id"]].append(
            dict(score=label["score_black"], family=family, tier=job["tier"], budget=result["budget_ms"])
        )
    rows = []
    excluded = Counter()
    for raw in db.execute("SELECT * FROM positions ORDER BY game_id,ply,id"):
        p = json.loads(raw["payload"])
        probe = json.loads(raw["probe"]) if raw["probe"] else None
        observations = labels.get(raw["id"], [])
        if not probe or probe.get("terminal") is not None or not observations:
            excluded["no_finite_label"] += 1
            continue
        if p["split"] != "train" and not raw["holdout_eligible"]:
            excluded["exposed_holdout"] += 1
            continue
        best = {}
        for item in observations:
            if item["family"] not in best or item["budget"] > best[item["family"]]["budget"]:
                best[item["family"]] = item
        turn = probe["turn"]
        if turn not in (-1, 1):
            raise ValueError("unknown side to move")
        game = games[p["game_id"]]
        outcome = None
        if not p.get("parent") and game.get("outcome_eligible"):
            black = {"BlackWins": 1.0, "WhiteWins": 0.0, "Draw": 0.5}[game["result"]]
            outcome = black if turn == 1 else 1 - black
        rows.append(
            dict(
                id=p["id"],
                game=p["game_id"],
                group=p["group"],
                split=p["split"],
                phase=p["phase"],
                stratum=p["stratum"],
                ply=p["ply"],
                path=p["path"],
                parent=p.get("parent"),
                turn=turn,
                canonical_position_hash=probe["canonical_position_hash"],
                state_history_hash=probe["state_history_hash"],
                material=probe["material_black"] * turn,
                range_two_movers=probe["range_two_movers"],
                scores=[x["score"] * turn for x in best.values()],
                observations=observations,
                outcome=outcome,
                game_path=game["path"],
                feature_schema_hash=probe["feature_schema_hash"],
            )
        )
    scale, calibration = temperature(rows)
    for r in rows:
        probs = [data.probability(x, scale) for x in r["scores"]]
        r["q"] = sum(probs) / len(probs)
        r["score"] = SCALE * math.log(max(1e-6, r["q"]) / max(1e-6, 1 - r["q"]))
        by_family = defaultdict(list)
        for x in r["observations"]:
            by_family[x["family"]].append(data.probability(x["score"], scale))
        drift = max(max(x) - min(x) for x in by_family.values())
        gap = max(probs) - min(probs)
        r["confidence"] = max(0.25, 1 / (1 + (gap / 0.15) ** 2 + (drift / 0.15) ** 2)) if len(probs) > 1 else 0.75
    # Global feature equivalence: no feature identity can cross a split. Conflicting
    # histories are withheld; repeated compatible rows get one deterministic vote.
    buckets = defaultdict(list)
    for r in rows:
        buckets[r["canonical_position_hash"]].append(r)
    clean = []
    for group in buckets.values():
        if len({r["split"] for r in group}) > 1 or max(r["q"] for r in group) - min(r["q"] for r in group) > 0.15:
            excluded["feature_split_or_history_conflict"] += len(group)
            continue
        chosen = min(group, key=lambda r: r["id"])
        # Average duplicate evidence, but do not propagate outcomes between histories.
        chosen["q"] = sum(r["q"] for r in group) / len(group)
        chosen["score"] = SCALE * math.log(max(1e-6, chosen["q"]) / max(1e-6, 1 - chosen["q"]))
        if len({r["outcome"] for r in group}) > 1:
            chosen["outcome"] = None
            excluded["ambiguous_duplicate_outcome"] += 1
        clean.append(chosen)
        excluded["compatible_duplicates"] += len(group) - 1
    clean.sort(key=lambda r: (r["game"], r["ply"], r["id"]))
    sampling_weights(clean)
    return clean, dict(excluded=dict(excluded), calibration=calibration, teacher_scale=scale, student_scale=SCALE)


def ranking_pairs(rows, scale):
    siblings = defaultdict(list)
    for i, r in enumerate(rows):
        if r["parent"] and r["split"] == "train":
            siblings[r["parent"]].append((i, r))
    pairs = []
    for root, children in siblings.items():
        candidates = []
        for a, (i, x) in enumerate(children):
            for j, y in children[a + 1 :]:
                # Child sign converts its probability/logit to absolute Black; either
                # common root perspective gives the same BCE after flipping both.
                px = x["q"] if x["turn"] == 1 else 1 - x["q"]
                py = y["q"] if y["turn"] == 1 else 1 - y["q"]
                if abs(px - py) < 0.10:
                    continue
                left = {(v["family"], v["budget"]): v["score"] for v in x["observations"]}
                right = {(v["family"], v["budget"]): v["score"] for v in y["observations"]}
                shared = left.keys() & right.keys()
                if len({k[0] for k in shared}) < 2 or len({k[1] for k in shared}) < 2:
                    continue
                if not all((left[k] - right[k]) * (px - py) > 0 for k in shared):
                    continue

                def logit(p):
                    return math.log(max(0.01, min(0.99, p)) / (1 - max(0.01, min(0.99, p))))

                candidates.append(dict(a=i, b=j, target=data.probability(logit(px) - logit(py), 1), root=root))
        for pair in candidates:
            pair["weight"] = 1 / len(candidates)
            pairs.append(pair)
    return pairs


def prepare(config):
    out = Path(config["out"]) / "dataset"
    marker = out / "dataset.json"
    if marker.exists():
        verify(out)
        return out
    out.mkdir(parents=True, exist_ok=True)
    db = connect(config["catalogue"])
    try:
        if not completed(db):
            raise ValueError("collection is incomplete or has failed jobs")
        snapshot = out / "evidence.sqlite.tmp"
        snapshot.unlink(missing_ok=True)
        destination = sqlite3.connect(snapshot)
        try:
            db.backup(destination)
        finally:
            destination.close()
        snapshot.replace(out / "evidence.sqlite")
    finally:
        db.close()
    db = connect(out / "evidence.sqlite")
    try:
        rows, report = select_rows(db)
    finally:
        db.close()
    counts = Counter(r["split"] for r in rows)
    groups = {s: {r["group"] for r in rows if r["split"] == s} for s in ("train", "validation", "test")}
    if any(counts[s] < 8 for s in groups) or any(
        groups[a] & groups[b] for a, b in [("train", "validation"), ("train", "test"), ("validation", "test")]
    ):
        raise ValueError("insufficient clean disjoint train/validation/test groups")
    pairs = ranking_pairs(rows, report["teacher_scale"])
    if not pairs:
        raise ValueError("no independently verified stable sibling pairs for regime D")
    schema = json.loads(subprocess.check_output([config["nnue_tool"], "schema"], text=True))
    baseline = json.loads(Path(config["parents"]["384"]).read_text())
    baseline["weights"].pop("nnue", None)
    atomic(out / "baseline.json", baseline)
    STOP.clear()
    oracle = None
    game = None
    offset = 0
    try:
        with (out / "features.bin").open("wb") as features:
            for r in rows:
                if r["game"] != game:
                    if oracle:
                        oracle.close()
                    if data.file_hash(r["game_path"]) != r["game"]:
                        raise ValueError("frozen source game changed")
                    oracle = Oracle(
                        dict(binary=config["binary"], request_timeout_seconds=900),
                        dict(game=r["game_path"], start_ply=1),
                        out,
                        str(out / "baseline.json"),
                    )
                    game = r["game"]
                result = oracle.call(dict(op="training_features", root_ply=r["ply"], path=r["path"]), lambda _: None)
                for key in ("canonical_position_hash", "state_history_hash", "turn", "feature_schema_hash"):
                    if result[key] != r[key]:
                        raise ValueError("feature replay mismatch: " + key)
                if (
                    schema["hash"] != r["feature_schema_hash"]
                    or abs(result["material_black"] * r["turn"] - r["material"]) > 0.1
                ):
                    raise ValueError("schema or parent material mismatch")
                values = array.array("I", result["us"] + result["them"])
                if os.sys.byteorder != "little":
                    values.byteswap()
                features.write(values.tobytes())
                r.update(offset=offset, us=len(result["us"]), them=len(result["them"]))
                offset += len(values)
        with (out / "samples.jsonl").open("w") as f:
            for r in rows:
                f.write(data.encoded(r) + "\n")
    finally:
        if oracle:
            oracle.close()
    atomic(out / "schema.json", schema)
    atomic(out / "pairs.json", pairs)
    atomic(
        out / "audit.json",
        dict(
            report,
            counts=dict(counts),
            groups={s: len(g) for s, g in groups.items()},
            pairs=len(pairs),
            mate_policy="unverified claims excluded",
            selection="fresh labels only; no saved-label supplement",
        ),
    )
    hashes = {
        name: data.file_hash(out / name)
        for name in ("features.bin", "samples.jsonl", "schema.json", "pairs.json", "audit.json", "evidence.sqlite")
    }
    atomic(
        marker,
        dict(
            version=1,
            files=hashes,
            scale=SCALE,
            collection_config_sha256=data.file_hash(config["collection_config"]),
            catalogue_sha256=data.file_hash(out / "evidence.sqlite"),
        ),
    )
    return out


def verify(out):
    identity = json.loads((out / "dataset.json").read_text())
    for name, sha in identity["files"].items():
        if data.file_hash(out / name) != sha:
            raise ValueError("frozen dataset changed: " + name)
    return identity


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    args = parser.parse_args()
    prepare(json.loads(args.config.read_text()))
