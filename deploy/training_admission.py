"""Admit each completed model through the running coordinator, without draining games."""

import argparse
import copy
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
        descriptor = checkpoint["weights"]["nnue"]
        export = source.parent / descriptor["file"]
        artifact_storage.snapshot_blob(export, export, descriptor["sha256"])
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
    policies = [a.read(e["model"]).get("search_defaults", {}).get("experiment", "baseline") for e in entries]
    engine_updates.require_search_support(bundle["engine_bin"], policies)
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
    report = out / "admission" / model.name
    tournament_tasks.ready(run)
    if a.read(model / "status.json").get("state") != "completed":
        raise ValueError("Model must finish training and held-out evaluation before admission")
    admit_checkpoints(run, [model], report)


def admit_checkpoints(run, models, report):
    """Shared content-pinned transaction for trained models or explicit search arms."""
    control = run / "analysis"
    report.mkdir(parents=True, exist_ok=True)
    tournament_tasks.ready(run)  # Fail before copying or publishing prerequisites.
    with (control / "update.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        check_storage(models, control, run)
        entries, bindings = freeze_models(models, control)
        bundle = engine_updates.active(a, run)
        validate_entries(bundle, entries, bindings)
        cfg = a.read(control / "config.json")
        cfg["models"] = merge_bindings(cfg["models"], bindings)
        # Publish dependencies before the scheduler can make the model eligible.
        # A crash leaves only unused bindings; retry sends the same immutable task.
        tournament_tasks.durable(control / "config.json", json.dumps(cfg, indent=2).encode())
        pinned = []
        for entry in entries:
            binding = bindings[entry["model"]]
            files = {entry["model"]: binding["sha256"]}
            nnue = a.read(entry["model"])["weights"]["nnue"]
            files[nnue["file"]] = nnue["sha256"]
            pinned.append(dict(entrant=entry, files=files))
        command = dict(kind="admit", entrants=pinned)
        intent = report / "live-intent.json"
        if intent.exists():
            task_id = a.read(intent)["task_id"]
        else:
            task_id = f"{time.time_ns():020d}-admit-{report.name.replace('.', '-')}"
            a.atomic(intent, dict(task_id=task_id))
        task_id, digest = tournament_tasks.submit(run, command, task_id)
        a.atomic(report / "status.json", dict(state="submitted", task_id=task_id, updated=time.time()))
    receipt = tournament_tasks.wait(run, task_id, digest)
    state = a.read(run / "state.json")
    for entry in entries:
        actual = next((e for e in state["entrants"] if e["id"] == entry["id"]), None)
        if actual is None or actual.get("model") != entry["model"] or actual.get("engine") != entry.get("engine"):
            raise RuntimeError("Applied receipt is not reflected in the tournament entrant state")
    a.atomic(report / "status.json", dict(state="admitted", task_id=task_id, receipt=receipt, updated=time.time()))


def prepare_search_variants(config, *, bundle=None):
    """Derive immutable search arms from registered baselines, sharing NNUE files.

    A string policy uses base_entrant; an object explicitly selects another
    base_entrant and experiment. Distinct (base, policy) pairs allow one search
    package to be tested on several networks in a single coordinator transaction.
    """
    run, out = Path(config["run"]), Path(config["out"])
    state = a.read(run / "state.json")
    variants = config["search_variants"]
    if not isinstance(variants, dict) or not variants:
        raise ValueError("Require distinct search policies")
    resolved = []
    seen = set()
    for name, spec in variants.items():
        if isinstance(spec, str):
            base_id, policy = config["base_entrant"], spec
        elif isinstance(spec, dict) and set(spec) == {"base_entrant", "experiment"}:
            base_id, policy = spec["base_entrant"], spec["experiment"]
        else:
            raise ValueError("Invalid search variant specification")
        if not isinstance(base_id, str) or not isinstance(policy, str) or not policy or policy == "baseline":
            raise ValueError("Invalid search entrant identity or policy")
        if Path(name).name != name or name in (".", "..", base_id):
            raise ValueError("Invalid search entrant identity or policy")
        if (base_id, policy) in seen:
            raise ValueError("Require distinct search policies per baseline")
        seen.add((base_id, policy))
        resolved.append((name, base_id, policy))
    bundle = engine_updates.verify(a, bundle) if bundle is not None else engine_updates.active(a, run)
    engine_updates.require_search_support(bundle["engine_bin"], [policy for _, _, policy in resolved])
    bases = {}
    # Validate every source before writing any candidate descriptor.
    for _, base_id, _ in resolved:
        if base_id in bases:
            continue
        base = next(e for e in state["entrants"] if e["id"] == base_id)
        if base.get("engine"):
            raise ValueError("Search variants require a current-engine NNUE baseline")
        source = Path(base["model"])
        checkpoint = a.read(source)
        if checkpoint.get("search_defaults", {}).get("experiment", "baseline") != "baseline":
            raise ValueError("Search baseline must use the unchanged policy")
        descriptor = checkpoint["weights"]["nnue"]
        network = (source.parent / descriptor["file"]).resolve()
        if a.file_digest(network) != descriptor["sha256"]:
            raise ValueError("Baseline NNUE hash mismatch")
        bases[base_id] = (base, source, checkpoint, network)
    models = []
    for name, base_id, policy in resolved:
        _, _, checkpoint, network = bases[base_id]
        model = copy.deepcopy(checkpoint)
        model["name"] = name
        model.setdefault("search_defaults", {})["experiment"] = policy
        model["weights"]["nnue"]["file"] = str(network)
        directory = out / name
        directory.mkdir(parents=True, exist_ok=True)
        target = directory / "model.json"
        if target.exists() and a.read(target) != model:
            raise ValueError("Search entrant definition changed: " + name)
        if not target.exists():
            a.atomic(target, model)
        models.append(directory)
    a.atomic(
        out / "search-provenance.json",
        dict(
            bases={
                ident: dict(entrant=base, sha256=a.file_digest(source), network_sha256=cp["weights"]["nnue"]["sha256"])
                for ident, (base, source, cp, _) in bases.items()
            },
            variants=variants,
        ),
    )
    return models


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--watch", action="store_true")
    parser.add_argument(
        "--search-variants",
        action="store_true",
        help="admit explicit search policies on a registered NNUE; no training status is fabricated",
    )
    args = parser.parse_args()
    config = a.read(args.config)
    out = Path(config["out"])
    if args.search_variants:
        if args.watch:
            parser.error("--watch is only for training admissions")
        tournament_tasks.ready(Path(config["run"]))
        models = prepare_search_variants(config)
        admit_checkpoints(Path(config["run"]), models, out / "admission" / "search-variants")
        return
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
