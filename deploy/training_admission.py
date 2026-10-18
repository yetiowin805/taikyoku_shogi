"""Admit each completed model through the running coordinator, without draining games."""

import argparse
import fcntl
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

import artifact_storage
import engine_updates
import storage_guard
import tournament_tasks
import tourney_analysis as a
from register_training_models import merge_bindings


def freeze_models(models, control):
    entries, bindings = [], {}
    (control / "models").mkdir(exist_ok=True)
    for directory in models:
        source = directory / "model.json"
        checkpoint = a.read(source)
        name = directory.name
        if checkpoint.get("name") != name or not checkpoint.get("weights", {}).get("nnue"):
            raise ValueError("Completed NNUE model identity missing: " + str(source))
        # Copy into the immutable model store, then use a stable admission path
        # independent of the training output's retention or future continuations.
        snapshot = a.snapshot_model(source, source.read_bytes(), control)
        frozen = a.read(snapshot["snapshot"])
        target = control / "admitted-models" / name / "model.json"
        target.parent.mkdir(parents=True, exist_ok=True)
        if target.exists() and a.read(target) != frozen:
            raise ValueError("Admitted model changed: " + name)
        if not target.exists():
            a.atomic(target, frozen)
        bindings[str(target)] = a.snapshot_model(target, target.read_bytes(), control)
        entries.append(dict(id=name, model=str(target)))
    return entries, bindings


def check_storage(models, control, run):
    """Reserve snapshot/transaction space before publishing model bindings."""
    policy = Path("/etc/taikyoku/storage.json")
    storage_guard.require_healthy(policy)
    store = Path(artifact_storage.settings().get("model_store", control / "models"))
    store.mkdir(parents=True, exist_ok=True)
    paths = ("state.json", "analysis/config.json", "analysis/manifest.json", "analysis/dual-label-config.json")
    # Backups plus atomic rewrites, with room for the small admission manifests.
    requirements = {control: 3 * sum((run / p).stat().st_size for p in paths if (run / p).exists()) + 1024**2}
    missing = {}
    for directory in models:
        descriptor = a.read(directory / "model.json")["weights"]["nnue"]
        if not (store / (descriptor["sha256"] + ".nnue")).exists():
            missing[descriptor["sha256"]] = (directory / descriptor["file"]).stat().st_size
    requirements[store] = requirements.get(store, 0) + sum(missing.values())
    devices = {}
    for path, size in requirements.items():
        device = os.stat(path).st_dev
        previous = devices.get(device, (path, 0))
        devices[device] = (path, previous[1] + size)
    rules = a.read(policy)["filesystems"] if policy.exists() else []
    for device, (path, needed) in devices.items():
        reserve = max((r["min_free_bytes"] for r in rules if os.stat(r["path"]).st_dev == device), default=0)
        if shutil.disk_usage(path).free < reserve + needed:
            raise ValueError("Insufficient snapshot/backup headroom above storage reserve: " + str(path))


def validate_entries(bundle, entries, bindings):
    for entry in entries:
        path = entry["model"]
        a.validate_model_binding(path, bindings[path])
        for command in (
            [bundle["engine_bin"], "tournament-game-validate", path],
            [bundle["analyzer_bin"], "--validate-model", path],
        ):
            subprocess.run(command, check=True, capture_output=True, text=True)


def admit(config, model):
    run, out = Path(config["run"]), Path(config["out"])
    control = run / "analysis"
    report = out / "admission" / model.name
    report.mkdir(parents=True, exist_ok=True)
    tournament_tasks.ready(run)  # Fail before copying or publishing prerequisites.
    if a.read(model / "status.json").get("state") != "completed":
        raise ValueError("Model must finish training and held-out evaluation before admission")
    with (control / "update.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        check_storage([model], control, run)
        entries, bindings = freeze_models([model], control)
        bundle = engine_updates.active(a, run)
        validate_entries(bundle, entries, bindings)
        cfg = a.read(control / "config.json")
        cfg["models"] = merge_bindings(cfg["models"], bindings)
        # Publish dependencies before the scheduler can make the model eligible.
        # A crash leaves only unused bindings; retry sends the same immutable task.
        tournament_tasks.durable(control / "config.json", json.dumps(cfg, indent=2).encode())
        entry = entries[0]
        binding = bindings[entry["model"]]
        files = {entry["model"]: binding["sha256"]}
        for artifact in binding.get("artifacts", []):
            files[artifact["original"]] = artifact["sha256"]
        command = dict(kind="admit", entrants=[dict(entrant=entry, files=files)])
        intent = report / "live-intent.json"
        if intent.exists():
            task_id = a.read(intent)["task_id"]
        else:
            task_id = f"{time.time_ns():020d}-admit-{model.name.replace('.', '-')}"
            a.atomic(intent, dict(task_id=task_id))
        task_id, digest = tournament_tasks.submit(run, command, task_id)
        a.atomic(report / "status.json", dict(state="submitted", task_id=task_id, updated=time.time()))
    receipt = tournament_tasks.wait(run, task_id, digest)
    state = a.read(run / "state.json")
    if not any(e == entry for e in state["entrants"]):
        # Serde may include engine:null in its canonical representation.
        actual = next((e for e in state["entrants"] if e["id"] == entry["id"]), None)
        if actual is None or actual.get("model") != entry["model"] or actual.get("engine") != entry.get("engine"):
            raise RuntimeError("Applied receipt is not reflected in the tournament entrant state")
    a.atomic(report / "status.json", dict(state="admitted", task_id=task_id, receipt=receipt, updated=time.time()))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--watch", action="store_true")
    args = parser.parse_args()
    config = a.read(args.config)
    out = Path(config["out"])
    names = config["admission_models"]
    if not names or len(set(names)) != len(names) or any(Path(n).name != n or n in (".", "..") for n in names):
        raise ValueError("Admission requires distinct model directory names")
    report = out / "admission"
    report.mkdir(exist_ok=True)
    with (report / "watcher.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        admitted = set()
        while True:
            failures = {}
            for name in names:
                if name in admitted:
                    continue
                model = out / name
                status = model / "status.json"
                if not status.exists() or a.read(status).get("state") != "completed":
                    continue
                try:
                    admit(config, model)
                except Exception as error:
                    a.atomic(report / name / "status.json", dict(state="failed", error=str(error), updated=time.time()))
                    failures[name] = str(error)
                    print(f"Admission failed for {name}: {error}", file=sys.stderr, flush=True)
                    continue
                admitted.add(name)
            waiting = [name for name in names if name not in admitted]
            a.atomic(
                report / "status.json",
                dict(
                    state="failed" if failures else "waiting_for_training" if waiting else "admitted",
                    failures=failures,
                    admitted=sorted(admitted),
                    waiting=waiting,
                    updated=time.time(),
                ),
            )
            if failures:
                raise RuntimeError("Some models could not be admitted; per-model errors are persisted")
            if not waiting or not args.watch:
                return
            time.sleep(10)


if __name__ == "__main__":
    main()
