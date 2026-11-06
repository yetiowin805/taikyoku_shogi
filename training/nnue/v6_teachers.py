"""Follow current order-neutral standings; pin a teacher set for each root family."""

import json
import shutil
import sys
import time
from pathlib import Path

from leader import select_leader

import v6_data as data
from run import atomic

POLICY = "highest-rated-distinct-evaluator-v1"


def freeze_model(source, out):
    source = source.resolve(strict=True)
    original_hash = data.file_hash(source)
    manifest = json.loads(source.read_text())
    target = out / "models" / original_hash
    target.mkdir(parents=True, exist_ok=True)
    network = manifest.get("weights", {}).get("nnue")
    identities = {}
    if network:
        blob = (source.parent / network["file"]).resolve(strict=True)
        if data.file_hash(blob) != network["sha256"]:
            raise ValueError("NNUE checksum mismatch: " + str(blob))
        dest = target / "network.nnue"
        if not dest.exists():
            shutil.copyfile(blob, dest.with_suffix(".tmp"))
            dest.with_suffix(".tmp").replace(dest)
        if data.file_hash(dest) != network["sha256"]:
            raise ValueError("frozen NNUE checksum mismatch")
        network["file"] = dest.name
        identities[str(dest)] = network["sha256"]
    path = target / "model.json"
    if path.exists():
        if json.loads(path.read_text()) != manifest:
            raise ValueError("frozen teacher descriptor changed")
    else:
        atomic(path, manifest)
    identities[str(path)] = data.file_hash(path)
    if data.file_hash(source) != original_hash:
        raise ValueError("teacher source changed while freezing")
    return dict(
        path=str(path),
        original=str(source),
        original_sha256=original_hash,
        sha256=identities[str(path)],
        network_sha256=network["sha256"] if network else None,
        identities=identities,
    )


def validate(model):
    for path, expected in model.get("identities", {}).items():
        if data.file_hash(path) != expected:
            raise ValueError("frozen teacher changed: " + path)


def family(manifest):
    weights = manifest.get("weights", {})
    if network := weights.get("nnue"):
        return "nnue:" + network["sha256"]
    return "handcrafted:" + data.hashed(weights)


def snapshot(config, previous=None):
    run = Path(config["teacher_run"])
    state = json.loads((run / "state.json").read_text())
    fit = json.loads((run / "order-neutral-ratings.json").read_text())
    if fit.get("games") != sum(row["status"] == "done" for row in state["slots"]):
        raise ValueError("standings and current completed-game count differ; retry after publication")
    registry = json.loads((run / "analysis/config.json").read_text())["models"]
    current = previous["models"]["primary"]["id"] if previous else None
    primary, _ = select_leader(state, fit, current)
    descriptors = {}

    def descriptor(entrant):
        name = entrant["id"]
        if name not in descriptors:
            source = Path(entrant["model"]).resolve(strict=True)
            binding = registry.get(str(source))
            if not binding or data.file_hash(source) != binding["sha256"]:
                raise ValueError("teacher lacks unchanged tournament binding: " + name)
            manifest = json.loads(source.read_text())
            if manifest.get("name") != name:
                raise ValueError("teacher name does not match entrant: " + name)
            descriptors[name] = manifest
        return descriptors[name]

    primary_family = family(descriptor(primary))
    eligible = sorted(
        (e for e in state["entrants"] if e["id"] in fit["ratings"] and e["id"] not in state.get("retired", [])),
        key=lambda e: (-fit["ratings"][e["id"]], e["id"]),
    )
    challenger = next((e for e in eligible if family(descriptor(e)) != primary_family), None)
    diagnostic = next((e for e in eligible if not descriptor(e).get("weights", {}).get("nnue")), None)
    if not challenger or not diagnostic:
        raise ValueError("standings need a distinct evaluator and a handcrafted diagnostic")
    models = {}
    for role, entrant in (("primary", primary), ("challenger", challenger), ("diagnostic", diagnostic)):
        # Historical binaries cannot be faithfully reproduced by this current-rules worker.
        if entrant.get("engine"):
            raise ValueError("selected teacher requires a different engine: " + entrant["id"])
        model = freeze_model(Path(entrant["model"]), Path(config["out"]))
        if model["original_sha256"] != registry[model["original"]]["sha256"]:
            raise ValueError("teacher binding changed while freezing: " + entrant["id"])
        model.update(id=entrant["id"], rating=fit["ratings"][entrant["id"]], family=family(descriptor(entrant)))
        models[role] = model
    return dict(policy=POLICY, selected_at=time.time(), models=models, fit=fit)


def bind(db, config, position):
    if not config.get("teacher_policy"):
        return config  # Small fixed fixtures and already prepared v1 collections.
    if config["teacher_policy"] != POLICY:
        raise ValueError("unknown teacher selection policy")
    root = position.get("parent") or position["id"]
    key = "teacher_root:" + root
    epoch = data.get_meta(db, key)
    if not epoch:
        selected = data.get_meta(db, "teacher_current") or config["initial_teachers"]
        now = time.time()
        retry_at = data.get_meta(db, "teacher_refresh_after") or 0
        if now >= retry_at:
            try:
                selected = snapshot(config, selected)
                report = dict(
                    state="selected", updated=now, teachers={k: v["id"] for k, v in selected["models"].items()}
                )
            except (OSError, ValueError, KeyError) as error:
                report = dict(state="retained_after_error", updated=now, error=str(error))
                print(f"teacher selection failed; retaining frozen teachers: {error}", file=sys.stderr, flush=True)
            atomic(Path(config["out"]) / "teacher-status.json", report)
            data.set_meta(db, "teacher_refresh_after", now + config["teacher_refresh_seconds"])
        epoch = data.hashed(selected)
        data.set_meta(db, "teacher_epoch:" + epoch, selected)
        data.set_meta(db, "teacher_current", selected)
        data.set_meta(db, key, epoch)
        # Binding survives a failed or interrupted search; later stages cannot reinterpret labels.
        db.commit()
    selected = data.get_meta(db, "teacher_epoch:" + epoch)
    if not selected:
        raise ValueError("missing frozen teacher epoch")
    return dict(config, models=selected["models"], teacher_epoch=epoch)
