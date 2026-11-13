"""V7 sensitivity pilot: bounded views composed from the shared v6 collector."""

import json
import os
from collections import defaultdict
from pathlib import Path

import v6_data as data


def select_games(records, config):
    """Keep complete split identities; sample one game per related group."""
    groups = defaultdict(list)
    for row in records.values():
        if (
            not row["quarantine"]
            and row["split"] != "test"
            and not (row["split"] == "validation" and row.get("inherited"))
        ):
            groups[(row["split"], row["group"])].append(row)
    selected = {}
    for split, limit in (
        ("train", config["sensitivity"]["train_groups"]),
        ("validation", config["sensitivity"]["validation_groups"]),
    ):
        keys = sorted((key for key in groups if key[0] == split), key=lambda key: data.hashed([config["seed"], key]))[
            :limit
        ]
        for key in keys:
            row = min(groups[key], key=lambda item: data.hashed([config["seed"], item["id"]]))
            selected[row["id"]] = row
    return selected


def membership(db, key, *members):
    row = json.loads(db.execute("SELECT payload FROM positions WHERE id=?", (key,)).fetchone()[0])
    row["pilot_views"] = sorted(set(row.get("pilot_views", [])) | set(members))
    db.execute("UPDATE positions SET payload=? WHERE id=?", (data.encoded(row), key))


def saved_move(row):
    if row is None:
        return None
    extra = row["data"]
    if isinstance(extra, dict) and "TwoStep" in extra:
        step = extra["TwoStep"]
        extra = dict(TwoStep=dict(intermediate=dict(file=step["intermediate_file"], rank=step["intermediate_rank"])))
    return {
        "from": dict(file=row["from_file"], rank=row["from_rank"]),
        "to": dict(file=row["to_file"], rank=row["to_rank"]),
        "promoted": row["promoted"],
        "data": extra,
    }


def add_child(db, config, root, path, members):
    payload = dict(
        root,
        path=path,
        parent=root.get("parent") or root["id"],
        stratum="alternative",
        selection="pilot_legal_path",
        saved=None,
        outcome=None,
        outcome_eligible=False,
    )
    payload.pop("id", None)
    payload.pop("pilot_views", None)
    key = data.add_position(db, payload)
    membership(db, key, *members)
    data.add_job(db, key, config["primary"], "child", config["budgets"]["broad"], 26)
    return key


def plan(db, config, rows):
    """S2 operates on all sampled roots; S3 takes one root per source group."""
    by_group = defaultdict(list)
    for row in rows:
        membership(db, row["id"], "S1:" + row["stratum"], "S3:" + row["stratum"])
        if row["stratum"] == "representative":
            membership(db, row["id"], "S2:anchor")
        data.add_job(db, row["id"], config["primary"], "settle", config["q_budget_ms"], 25)
        by_group[row["group"]].append(row)
    for group in sorted(by_group):
        root = min(by_group[group], key=lambda r: data.hashed([config["seed"], "explore", r["id"]]))
        data.add_job(db, root["id"], config["primary"], "explore", 0, 30)


def settle_steps(db, config, position, members):
    result = yield (
        dict(
            op="collection_settle",
            root_ply=position["ply"],
            path=position["path"],
            depth=6,
            time_ms=config["q_budget_ms"],
        ),
        lambda _: None,
    )
    line = result.get("settled_line")
    if line is not None and not result.get("quiescence", {}).get("aborted", False):
        if not line:
            membership(db, position["id"], *members)
        else:
            add_child(db, config, position, position["path"] + line, members)
    return dict(settled=line is not None, changed=bool(line), probe=result, label=None)


def execute_steps(db, config, job, position):
    if job["tier"] == "settle":
        return (yield from settle_steps(db, config, position, ["S2:settled_" + position["stratum"]]))
    source = db.execute(
        "SELECT result FROM jobs WHERE position_id=? AND teacher=? AND tier='broad' AND status='completed'",
        (position["id"], config["primary"]),
    ).fetchone()
    label = json.loads(source[0])["label"]
    game = json.loads(db.execute("SELECT payload FROM games WHERE id=?", (position["game_id"],)).fetchone()[0])
    # The saved move at this root is the move at ply, while saved evaluations refer to ply-1.
    record = json.loads(Path(game["path"]).read_text())
    played = record["moves"][position["ply"] - 1] if position["ply"] <= len(record["moves"]) else None
    played = saved_move(played)
    candidates = [label["best"], *[x["move"] for x in label["candidates"]]]
    teacher_move = next((mv for mv in candidates if mv is not None and mv != played), None)
    seed = int(data.hashed([config["seed"], position["id"], "random"])[0:16], 16)
    random_result = yield (
        dict(
            op="collection_moves",
            root_ply=position["ply"],
            path=[],
            sampling_seed=seed,
            candidates=[played] if played else [],
        ),
        lambda _: None,
    )
    moves = [("teacher", teacher_move), ("random", random_result.get("move"))]
    reports = []
    for arm, move in moves:
        if move is None or move == played or any(move == x["initial"] for x in reports):
            continue
        path = [move]
        members = ["S3:alternative"]
        if arm == "teacher":
            members.append("S1:alternative")
        key = add_child(db, config, position, path, members)
        child = json.loads(db.execute("SELECT payload FROM positions WHERE id=?", (key,)).fetchone()[0])
        if arm == "teacher":
            yield from settle_steps(db, config, child, ["S2:settled_alternative"])
        for _ in range(config["sensitivity"].get("continuation_plies", 3)):
            events = []
            final = yield (
                dict(op="search", root_ply=position["ply"], path=path, depth=64, time_ms=config["budgets"]["broad"]),
                events.append,
            )
            finished = [event for event in events if event.get("event") == "iteration"]
            if not finished or final.get("terminal") is not None:
                break
            best = max(finished, key=lambda event: event["depth"]).get("best")
            if best is None:
                break
            path = [*path, best]
        endpoint = dict(child, path=path)
        report = yield from settle_steps(db, config, endpoint, ["S3:alternative"])
        if report["settled"] and not report["changed"]:
            add_child(db, config, position, path, ["S3:alternative"])
        reports.append(dict(arm=arm, initial=move, path=path, settled=report["settled"]))
    return dict(arms=reports, random_categories=random_result.get("category_counts"), label=None)


def cutoff(db, config):
    """Retain only families with all mandatory position-creation/label jobs complete."""
    excluded = set()
    for row in db.execute(
        "SELECT p.payload,j.tier FROM jobs j JOIN positions p ON p.id=j.position_id "
        "WHERE j.status IN ('pending','running')"
    ):
        if row["tier"] in ("broad", "child", "settle", "explore"):
            p = json.loads(row["payload"])
            excluded.add(p.get("parent") or p["id"])
    db.execute(
        "UPDATE jobs SET status='cancelled',error='pilot collection budget' WHERE status IN ('pending','running')"
    )
    data.set_meta(db, "pilot_cutoff", dict(excluded_roots=sorted(excluded), reason="elapsed_budget"))
    for key in ("audit_planned", "deep_planned"):
        if data.get_meta(db, key) is None:
            data.set_meta(db, key, dict(reason="pilot_budget_before_stage"))
    db.commit()


def build_views(source, out):
    """Shared immutable feature bytes, with explicit per-view sampling masses."""
    import v6_dataset as dataset
    from train import atomic_json

    source = Path(source)
    identity = dataset.verify(source)
    rows = [json.loads(line) for line in (source / "samples.jsonl").read_text().splitlines()]
    policies = {
        "S1": dict(representative=0.75, challenge=0.20, alternative=0.05),
        "S2": dict(representative=0.25, settled_representative=0.50, settled_challenge=0.20, settled_alternative=0.05),
        "S3": dict(representative=0.50, challenge=0.10, alternative=0.40),
    }
    manifests = {}
    for name, masses in policies.items():
        target = Path(out) / name
        target.mkdir(parents=True, exist_ok=True)
        selected = []
        for original in rows:
            roles = {view.split(":", 1)[1] for view in original["pilot_views"] if view.startswith(name + ":")}
            if original["split"] != "train" and "S1:representative" in original["pilot_views"]:
                roles.add("representative")
            for role in sorted(roles):
                if role == "anchor":
                    role = "representative"
                if role not in masses:
                    continue
                row = dict(original, stratum=role, confidence=original["raw_confidence"])
                selected.append(row)
        training = [r for r in selected if r["split"] == "train"]
        if not training or not any(r["split"] == "validation" and r["stratum"] == "representative" for r in selected):
            raise ValueError(f"{name} lacks training or common representative validation")
        required = "settled_" if name == "S2" else "alternative" if name == "S3" else "representative"
        if not any(r["stratum"].startswith(required) for r in training):
            raise ValueError(f"{name} has no evidence for its intervention")
        dataset.sampling_weights(selected, masses)
        for file in identity["files"]:
            if file in ("samples.jsonl", "audit.json"):
                continue
            if not (target / file).exists():
                os.link(source / file, target / file)
        with (target / "samples.jsonl").open("w") as f:
            for row in selected:
                f.write(data.encoded(row) + "\n")
        report = dict(
            view=name,
            counts={s: sum(r["split"] == s for r in selected) for s in ("train", "validation", "test")},
            unique_training=len({r["id"] for r in training}),
            groups=len({r["group"] for r in training}),
            requested_masses=masses,
            actual_masses={s: sum(r["sampling_weight"] for r in training if r["stratum"] == s) for s in masses},
            outcome_mass=sum(r["sampling_weight"] for r in training if r["outcome"] is not None),
        )
        atomic_json(target / "audit.json", report)
        hashes = {file: data.file_hash(target / file) for file in identity["files"]}
        atomic_json(target / "dataset.json", dict(identity, files=hashes, pilot_view=name))
        manifests[name] = dict(path=str(target), **report)
    atomic_json(Path(out) / "views.json", manifests)
    return manifests
