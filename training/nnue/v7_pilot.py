"""Deployable, resumable sixteen-hour v7 sensitivity pilot; never admits models."""

import argparse
import fcntl
import json
import os
import signal
import subprocess
import time
from pathlib import Path

import v6_collect as collect
import v6_data as data
import v6_teachers as teachers

VARIANTS = (("A", "S1"), ("A", "S2"), ("A", "S3"), ("Ao", "S1"), ("C", "S1"), ("Cu", "S1"), ("E", "S1"))


def tasks(config):
    out = Path(config["out"])
    budget = out / "pilot-budget.json"
    windows = json.loads(budget.read_text())["windows"] if budget.exists() else 12
    common = dict(
        width=256, parent=config["parents"]["256"], pilot_windows=windows, deadline=config["training_deadline"]
    )
    jobs = [
        dict(
            name="prelude-256-A",
            dependencies=[],
            fit=dict(common, regime="A", prelude=True, dataset=str(out / "views/S1"), seed=20261112),
        )
    ]
    for regime, view in VARIANTS:
        for seed in (20261112, 20261113):
            jobs.append(
                dict(
                    name=f"pilot-256-{regime}-{view}-{seed}",
                    dependencies=["prelude-256-A"],
                    fit=dict(
                        common,
                        regime=regime,
                        retain_material=regime in ("C", "Cu"),
                        dataset=str(out / "views" / view),
                        pilot_probe_dataset=str(out / "dataset"),
                        seed=seed,
                        start=str(out / "prelude-256-A/start.pt"),
                    ),
                )
            )
    # Paired seeds and controls first; an interrupted run exposes missing comparisons.
    return jobs


def calibrate_training(config):
    """Freeze one common budget after the shared prelude, before any treatment starts."""
    out = Path(config["out"])
    path = out / "pilot-budget.json"
    if path.exists() or not (out / "prelude-256-A/complete.json").exists():
        return
    metrics = json.loads((out / "prelude-256-A/metrics.json").read_text())
    seconds = sorted(r["seconds"] for r in metrics)[len(metrics) // 2]
    available = max(0, config["training_deadline"] - time.time())
    # Four waves at four workers. Reserve 25% for exports, probes and adaptive audits.
    windows = next((n for n in (12, 10, 8) if 4 * n * seconds * 1.25 <= available), 8)
    collect.atomic(
        path,
        dict(
            windows=windows,
            prelude_window_seconds=seconds,
            available_seconds=available,
            predicted_seconds=4 * windows * seconds * 1.25,
            minimum_budget_at_risk=4 * 8 * seconds * 1.25 > available,
        ),
    )


def prepare(args):
    collect.prepare(args)
    out = args.out.resolve()
    config = json.loads((out / "config.json").read_text())
    config.update(
        workers=6,
        lease_indices=[7, 1, 3, 4, 5, 6],
        representative=4,
        challenge=2,
        pilot_roots=32,
        pilot_challenger=16,
        audit_roots=192,
        deep_roots=32,
        branch_roots=0,
        branch_audit_roots=0,
        branch_deep_roots=0,
        sensitivity=dict(train_groups=384, validation_groups=96, collection_deadline=0),
    )
    state = json.loads((out / "source-state.json").read_text())
    fit = json.loads((args.run / "order-neutral-ratings.json").read_text())
    eligible = []
    for entrant in state["entrants"]:
        if entrant["id"] in state.get("retired", []) or entrant.get("engine"):
            continue
        descriptor = json.loads(Path(entrant["model"]).read_text())
        net = descriptor.get("weights", {}).get("nnue", {})
        if net.get("width") == 256 and net.get("output_mode", "residual") == "residual":
            eligible.append(entrant)
    parent, rating = teachers.select_leader(dict(state, entrants=eligible), fit)
    frozen = teachers.freeze_model(Path(parent["model"]), out / "parent")
    collect.atomic(
        out / "parent-selection.json",
        dict(
            id=parent["id"],
            rating=rating,
            model=frozen,
            policy="current order-neutral leader among active residual 256-wide models",
        ),
    )
    data.validate_config(config)
    collect.atomic(out / "config.json", config)
    pipeline = dict(
        out=str(out / "training"),
        catalogue=str(out / "catalogue.sqlite"),
        collection_config=str(out / "config.json"),
        control=config["control"],
        parents={"256": frozen["path"]},
        identities=dict(
            frozen["identities"],
            **{
                config["binary"]: data.file_hash(config["binary"]),
                str(args.nnue_tool.resolve()): data.file_hash(args.nnue_tool),
            },
        ),
        lease_indices=config["lease_indices"],
        binary=config["binary"],
        nnue_tool=str(args.nnue_tool.resolve()),
        python=str(args.python.resolve()),
        sensitivity=True,
        training_deadline=0,
        max_parallel=4,
    )
    collect.atomic(out / "pipeline.json", pipeline)
    collect.atomic(
        out / "prepared.json",
        dict(
            revision=args.revision,
            parent=parent["id"],
            teachers=config["initial_teachers"]["models"],
            planned_hours=16,
            input_schema="legacy v1; full v7 identity inputs deferred",
            admission=False,
        ),
    )


def stage_state(out, stage):
    if stage == "diagnostics":
        report = json.loads((out / "report.json").read_text())
        return "incomplete" if report["incomplete_runs"] else "completed"
    status = out / ("status.json" if stage == "collection" else "training/status.json")
    state = json.loads(status.read_text())["state"]
    if state not in ("completed", "pilot_budget"):
        raise ValueError(f"{stage} exited without completing or reaching its recorded pilot budget: {state}")
    if stage == "collection":
        import v6_dataset

        db = v6_dataset.connect(out / "catalogue.sqlite")
        try:
            if not v6_dataset.completed(db):
                raise ValueError("collection has no complete usable catalogue; training did not start")
        finally:
            db.close()
    return state


def run(path):
    out = path.resolve()
    if not (out / "prepared.json").exists():
        raise ValueError("pilot preparation did not complete; pilot did not start")
    with (out / "pilot.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        timing = out / "timing.json"
        if not timing.exists():
            start = time.time()
            collect.atomic(
                timing,
                dict(
                    start=start,
                    collection_deadline=start + 8 * 3600,
                    training_deadline=start + 14 * 3600,
                    deadline=start + 16 * 3600,
                ),
            )
        times = json.loads(timing.read_text())
        config = json.loads((out / "config.json").read_text())
        config["sensitivity"]["collection_deadline"] = times["collection_deadline"]
        collect.atomic(out / "config.json", config)
        pipeline = json.loads((out / "pipeline.json").read_text())
        pipeline["training_deadline"] = times["training_deadline"]
        collect.atomic(out / "pipeline.json", pipeline)
        child = None

        def stop(*_):
            if child and child.poll() is None:
                child.terminate()
            raise InterruptedError("pilot paused; durable stages can resume")

        signal.signal(signal.SIGTERM, stop)
        signal.signal(signal.SIGINT, stop)
        try:
            for stage, module, argument in (
                ("collection", "v6_collect.py", ["run", str(out / "config.json")]),
                ("training", "v6_pipeline.py", [str(out / "pipeline.json")]),
                ("diagnostics", "v7_pilot_report.py", [str(out)]),
            ):
                marker = out / (stage + "-done.json")
                if marker.exists():
                    continue
                collect.atomic(out / "pilot-status.json", dict(state=stage, updated=time.time(), **times))
                with (out / (stage + ".log")).open("ab") as log:
                    child = subprocess.Popen(
                        [pipeline["python"], str(Path(__file__).with_name(module)), *argument],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        env=dict(os.environ, OMP_NUM_THREADS="1", MKL_NUM_THREADS="1", OPENBLAS_NUM_THREADS="1"),
                    )
                    code = child.wait()
                if code:
                    raise RuntimeError(f"{stage} failed ({code}); see {stage}.log; subsequent stages did not start")
                collect.atomic(marker, dict(completed=time.time(), state=stage_state(out, stage)))
            collect.atomic(
                out / "pilot-status.json",
                dict(
                    state="completed_with_incomplete_runs"
                    if stage_state(out, "diagnostics") == "incomplete"
                    else "completed",
                    updated=time.time(),
                    **times,
                ),
            )
        except Exception as error:
            collect.atomic(
                out / "pilot-status.json",
                dict(
                    state="paused" if isinstance(error, InterruptedError) else "failed",
                    error=str(error),
                    updated=time.time(),
                    **times,
                ),
            )
            raise
        finally:
            if child and child.poll() is None:
                child.terminate()
                child.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    for name in ("out", "run", "source-repo", "binary", "nnue-tool", "python"):
        prep.add_argument("--" + name, type=Path, required=True)
    prep.add_argument("--parent-samples", type=Path, action="append", required=True)
    prep.add_argument("--revision", required=True)
    prep.add_argument("--lease-index", type=int, default=7)
    commands.add_parser("run").add_argument("out", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args)
    else:
        run(args.out)
