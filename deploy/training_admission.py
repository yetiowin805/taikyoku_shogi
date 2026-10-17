"""Admit completed training runs automatically, after existing games drain.

The standalone watcher also attaches to already-running immutable trainers.
It never edits a training recipe, selects an intermediate checkpoint, retires
an existing entrant, or changes the game engine/time control.
"""

import argparse
import copy
import fcntl
import os
import shutil
import signal
import subprocess
import time
from contextlib import ExitStack
from pathlib import Path

import artifact_storage
import engine_updates
import storage_guard
import tourney_analysis as a
from register_training_models import merge_bindings


def completed(models):
    return all((p / "status.json").exists() and a.read(p / "status.json").get("state") == "completed" for p in models)


def add_entrants(state, entrants):
    """Append identities only; preserve every result, rating, bracket and retirement."""
    result = copy.deepcopy(state)
    existing = {e["id"]: e for e in result["entrants"]}
    for entry in entrants:
        previous = existing.get(entry["id"])
        if previous is not None:
            if any(previous.get(k) != entry.get(k) for k in ("model", "engine")):
                raise ValueError("Entrant identity changed: " + entry["id"])
            if entry["id"] in result.get("retired", []):
                raise ValueError("Completed model is already retired: " + entry["id"])
        else:
            result["entrants"].append(entry)
            existing[entry["id"]] = entry
    return result


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
    """Reserve snapshot/transaction space before copying or draining games."""
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


def claim_idle(control, leases, stack, count):
    """Hold each CPU only after its current game/analysis releases the lease."""
    for i in range(count):
        if i in leases:
            continue
        f = (control / f"cpu-{i}.lock").open("a+")
        try:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            f.close()
        else:
            leases[i] = stack.enter_context(f)


def service(action, name):
    subprocess.run(["systemctl", action, name], check=True)


def validate_entries(bundle, entries, bindings):
    for entry in entries:
        path = entry["model"]
        a.validate_model_binding(path, bindings[path])
        for command in (
            [bundle["engine_bin"], "tournament-game-validate", path],
            [bundle["analyzer_bin"], "--validate-model", path],
        ):
            subprocess.run(command, check=True, capture_output=True, text=True)


def admit(config, models):
    run, out = Path(config["run"]), Path(config["out"])
    control, report = run / "analysis", out / "admission"
    report.mkdir(exist_ok=True)
    status_path = report / "status.json"

    def status(state, **fields):
        a.atomic(status_path, dict(state=state, updated=time.time(), **fields))

    coordinator = config.get("coordinator_service", "taikyoku-coordinator.service")
    analysis = "taikyoku-dual-labels.service"
    paused = False
    with ExitStack() as stack:
        lock = stack.enter_context((control / "training-admission.lock").open("a+"))
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if not completed(models):
            raise ValueError("All requested models must finish training and held-out evaluation before admission")
        cfg = a.read(control / "config.json")
        if len(cfg["cpus"]) != 8 or cfg.get("sidecar") != "none":
            raise ValueError("Automatic admission requires the eight-CPU external-sidecar coordinator")
        check_storage(models, control, run)
        entries, bindings = freeze_models(models, control)
        state = a.read(run / "state.json")
        merged = add_entrants(state, entries)  # Identity conflicts fail before draining games.
        previous = a.read(status_path) if status_path.exists() else {}
        if previous.get("state") == "admitted" and merged == state:
            # Also recover a hard kill after the admission commit but before
            # cleanup restored analysis. Starting an active unit is idempotent.
            service("start", analysis)
            return
        bundle = engine_updates.active(a, run)
        validate_entries(bundle, entries, bindings)
        intent_path = report / "intent.json"
        if intent_path.exists():
            if a.read(intent_path)["entrants"] != entries:
                raise ValueError("Admission transaction belongs to different models")
        else:
            a.atomic(intent_path, dict(entrants=entries, created=time.time()))
        try:
            paused = True
            service("stop", analysis)
            own = stack.enter_context((control / "dual-label.lock").open("a+"))
            fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
            # No wall-clock deadline: arbitrarily long legitimate games finish.
            leases = {}
            with (control / "allocation.lock").open("a+") as gate:
                # Block new claims, not existing games. Training has returned
                # its leases and analysis is paused. Without this gate a game
                # worker could immediately reclaim its CPU between our polls.
                fcntl.flock(gate, fcntl.LOCK_EX)
                while len(leases) < 8:
                    claim_idle(control, leases, stack, 8)
                    status("draining", reserved_cpus=sorted(leases), entrants=[e["id"] for e in entries])
                    if len(leases) < 8:
                        time.sleep(2)
            update = stack.enter_context((control / "update.lock").open("a+"))
            fcntl.flock(update, fcntl.LOCK_EX | fcntl.LOCK_NB)
            storage_guard.require_healthy("/etc/taikyoku/storage.json")
            cfg = a.read(control / "config.json")
            current_bundle = engine_updates.active(a, run)
            if current_bundle != bundle:
                validate_entries(current_bundle, entries, bindings)
            state = a.read(run / "state.json")
            if any(s["status"] == "running" for s in state["slots"]):
                raise ValueError("All CPU leases held but games still running; refusing admission")
            status("publishing", entrants=[e["id"] for e in entries])
            service("stop", coordinator)
            # Re-read the final checkpoint. On recovery, append idempotently to
            # current state; never restore an old ledger over newer game results.
            state = a.read(run / "state.json")
            updated = add_entrants(state, entries)
            files = {
                "analysis/config.json": dict(cfg, models=merge_bindings(cfg["models"], bindings)),
                "analysis/manifest.json": dict(entrants=updated["entrants"]),
                "state.json": updated,
            }
            dual = control / "dual-label-config.json"
            if dual.exists():
                value = a.read(dual)
                value["old"]["models"] = merge_bindings(value["old"]["models"], bindings)
                files["analysis/dual-label-config.json"] = value
            backup = report / f"before-{time.time_ns()}"
            for relative in files:
                source, target = run / relative, backup / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                if source.exists():
                    target.write_bytes(source.read_bytes())
            # A crash between writes is repaired by the same idempotent merge.
            # The coordinator remains stopped until all files are published.
            for relative, value in files.items():
                a.atomic(run / relative, value)
            service("start", coordinator)
            while True:
                meta = a.read(control / "supervisor.json")
                if a.alive(meta) and meta.get("state") == "running":
                    break
                result = subprocess.run(["systemctl", "is-active", "--quiet", coordinator])
                if result.returncode:
                    raise RuntimeError("Coordinator did not restart after admission; transaction is resumable")
                time.sleep(1)
            status("admitted", entrants=[e["id"] for e in entries], backup=str(backup))
        except BaseException as error:
            status("failed", error=str(error))
            raise
        finally:
            # Release the analysis lock and CPU leases before restarting its service.
            stack.close()
            if paused:
                service("start", analysis)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--watch", action="store_true")
    args = parser.parse_args()
    config = a.read(args.config)
    out = Path(config["out"])
    names = config.get("admission_models", [f"NNUE_W512_v5{s}" for s in "abc"])
    if not names or len(set(names)) != len(names) or any(Path(n).name != n or n in (".", "..") for n in names):
        raise ValueError("Admission requires distinct model directory names under the training output")
    models = [out / name for name in names]

    def stop(*_):
        raise InterruptedError("Automatic admission stopped; progress is resumable")

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    while args.watch:
        if completed(models):
            # The old trainer publishes completion just before releasing leases
            # and restarting analysis. Wait for its service to finish cleanup too.
            trainer = config.get("training_service", "taikyoku-v5-training.service")
            phase = subprocess.check_output(
                ["systemctl", "show", trainer, "-p", "ActiveState", "--value"], text=True
            ).strip()
            if phase in ("inactive", "failed"):
                break
        report = out / "admission/status.json"
        report.parent.mkdir(exist_ok=True)
        previous = a.read(report) if report.exists() else {}
        if previous.get("state") not in ("admitted", "draining", "publishing"):
            a.atomic(report, dict(state="waiting_for_training", models=names, updated=time.time()))
        time.sleep(30)
    admit(config, models)


if __name__ == "__main__":
    main()
