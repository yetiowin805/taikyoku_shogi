"""Full v7 campaign: versioned inputs, three views, five residual recipes, two widths."""

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
from v6_runtime import LeasePool
from v7_pilot import check_training_python, training_python

WIDTHS = (256, 384)
REGIMES = ("A", "Ao", "C", "Cu", "E")
VIEWS = ("S1", "S2", "S3")


def tasks(config):
    out = Path(config["out"])
    jobs = []
    for width in WIDTHS:
        jobs.append(
            dict(
                name=f"prelude-{width}-A",
                dependencies=[],
                fit=dict(
                    width=width,
                    regime="A",
                    prelude=True,
                    full_v7=True,
                    parent=config["parents"][str(width)],
                    dataset=str(out / "views/S1"),
                    seed=20261113,
                ),
            )
        )
    for view in VIEWS:
        for regime in REGIMES:
            for width in WIDTHS:
                jobs.append(
                    dict(
                        name=f"NNUE_W{width}_v7_{view}_{regime}",
                        dependencies=[f"prelude-{width}-A"],
                        fit=dict(
                            width=width,
                            regime=regime,
                            full_v7=True,
                            retain_material=True,
                            parent=config["parents"][str(width)],
                            start=str(out / f"prelude-{width}-A/start.pt"),
                            dataset=str(out / "views" / view),
                            pilot_probe_dataset=str(out / "dataset"),
                            seed=20261113,
                        ),
                    )
                )
    return jobs


def prepare(args):
    check_training_python(args.python)
    collect.prepare(args)
    out = args.out.resolve()
    config = json.loads((out / "config.json").read_text())
    config.update(
        full_v7=True,
        feature_version=2,
        workers=6,
        lease_indices=[7, 1, 3, 4, 5, 6],
        representative=8,
        challenge=2,
        roots_per_group=40,
        explore_roots_per_group=4,
        sensitivity=dict(continuation_plies=3),
        pilot_roots=256,
        pilot_challenger=32,
        audit_roots=0,
        deep_roots=0,
        branch_roots=0,
        branch_audit_roots=0,
        branch_deep_roots=0,
    )
    state = json.loads((out / "source-state.json").read_text())
    ratings = json.loads((args.run / "order-neutral-ratings.json").read_text())
    parents = {}
    selected = {}
    identities = dict(config["identities"])
    for width in WIDTHS:
        eligible = []
        for entrant in state["entrants"]:
            if entrant["id"] in state.get("retired", []) or entrant.get("engine"):
                continue
            model = json.loads(Path(entrant["model"]).read_text())
            net = model.get("weights", {}).get("nnue", {})
            if net.get("width") == width and net.get("output_mode", "residual") == "residual":
                eligible.append(entrant)
        leader, rating = teachers.select_leader(dict(state, entrants=eligible), ratings)
        frozen = teachers.freeze_model(Path(leader["model"]), out / "original-parents")
        identities.update(frozen["identities"])
        parents[str(width)] = frozen["path"]
        selected[str(width)] = dict(id=leader["id"], rating=rating, model=frozen)
    nnue_tool = str(args.nnue_tool.resolve())
    identities[nnue_tool] = data.file_hash(nnue_tool)
    data.validate_config(config)
    collect.atomic(out / "config.json", config)
    collect.atomic(out / "parent-selection.json", selected)
    pipeline = dict(
        out=str(out / "training"),
        catalogue=str(out / "catalogue.sqlite"),
        collection_config=str(out / "config.json"),
        control=config["control"],
        parents={str(w): str(out / f"parent-v2-{w}/model.json") for w in WIDTHS},
        original_parents=parents,
        identities=identities,
        lease_indices=config["lease_indices"],
        binary=config["binary"],
        nnue_tool=nnue_tool,
        python=training_python(args.python),
        full_v7=True,
        feature_version=2,
        sensitivity=True,
        max_parallel=args.trainers,
        window_draws=65536,
    )
    collect.atomic(out / "pipeline.json", pipeline)
    collect.atomic(
        out / "prepared.json",
        dict(
            revision=args.revision,
            candidate_games=sum(s["status"] == "done" for s in state["slots"]),
            parents=selected,
            input_schema=2,
            recipes=list(REGIMES),
            widths=list(WIDTHS),
            views=list(VIEWS),
            fixed_material=True,
            admission=False,
        ),
    )


def migrate_parents(out, config, stopped):
    pool = LeasePool(Path(config["control"]), collect.BoundaryLeases, reserve=1)
    lease = None
    previous_affinity = os.sched_getaffinity(0)
    try:
        while lease is None:
            if stopped():
                raise InterruptedError("migration paused while waiting for a CPU lease")
            acquired = pool.update(config["lease_indices"], collect.allocation_busy(Path(config["control"])))
            if acquired:
                cpu, lease = acquired
                os.sched_setaffinity(0, {cpu})
                pool.close()
            else:
                time.sleep(1)
        from v7_migrate import migrate

        schemas = [
            json.loads(subprocess.check_output([config["nnue_tool"], "schema", str(v)], text=True)) for v in (1, 2)
        ]
        for version, schema in enumerate(schemas, 1):
            collect.atomic(out / f"schema-v{version}.json", schema)
        for width in WIDTHS:
            result = migrate(config["original_parents"][str(width)], *schemas, out / f"parent-v2-{width}")
            model = Path(config["parents"][str(width)])
            config["identities"][str(model)] = result["model_sha256"]
            config["identities"][str(model.parent / result["network"])] = result["network_sha256"]
    finally:
        pool.close()
        if lease:
            lease.close()
        os.sched_setaffinity(0, previous_affinity)


def run(path):
    out = path.resolve()
    if not (out / "prepared.json").exists():
        raise ValueError("full v7 preparation did not complete; campaign did not start")
    with (out / "campaign.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        config = json.loads((out / "pipeline.json").read_text())
        child = None
        stopping = False

        def stop(*_):
            nonlocal stopping
            stopping = True
            if child and child.poll() is None:
                child.terminate()

        signal.signal(signal.SIGTERM, stop)
        signal.signal(signal.SIGINT, stop)
        try:
            check_training_python(config["python"])
            if not (out / "migration-done.json").exists():
                collect.atomic(out / "campaign-status.json", dict(state="migration", updated=time.time()))
                migrate_parents(out, config, lambda: stopping)
                collect.atomic(out / "pipeline.json", config)
                collect.atomic(out / "migration-done.json", dict(completed=time.time()))
            for stage, module, arguments in (
                ("collection", "v6_collect.py", ["run", str(out / "config.json")]),
                ("training", "v6_pipeline.py", [str(out / "pipeline.json")]),
            ):
                if stopping:
                    raise InterruptedError("full v7 campaign paused")
                marker = out / (stage + "-done.json")
                if marker.exists():
                    continue
                collect.atomic(out / "campaign-status.json", dict(state=stage, updated=time.time()))
                with (out / (stage + ".log")).open("ab") as log:
                    child = subprocess.Popen(
                        [config["python"], str(Path(__file__).with_name(module)), *arguments],
                        stdout=log,
                        stderr=subprocess.STDOUT,
                        env=dict(os.environ, OMP_NUM_THREADS="1", MKL_NUM_THREADS="1", OPENBLAS_NUM_THREADS="1"),
                    )
                    code = child.wait()
                if stopping:
                    raise InterruptedError("full v7 campaign paused")
                if code:
                    raise RuntimeError(f"{stage} failed ({code}); inspect {stage}.log; subsequent stages did not start")
                status_path = out / ("status.json" if stage == "collection" else "training/status.json")
                status = json.loads(status_path.read_text())
                if status["state"] != "completed":
                    raise RuntimeError(f"{stage} exited without completing: {status['state']}")
                collect.atomic(marker, dict(completed=time.time()))
            ready = json.loads((out / "training/ready.json").read_text())
            if len(ready["models"]) != 30:
                raise ValueError("full matrix is incomplete")
            collect.atomic(
                out / "campaign-status.json",
                dict(
                    state="trained_awaiting_playing_evaluation",
                    models=ready["models"],
                    updated=time.time(),
                    admission=False,
                    fresh_test_used=False,
                ),
            )
        except Exception as error:
            collect.atomic(
                out / "campaign-status.json",
                dict(
                    state="paused" if isinstance(error, InterruptedError) else "failed",
                    error=str(error),
                    updated=time.time(),
                ),
            )
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prep = commands.add_parser("prepare")
    for name in ("out", "run", "source-repo", "binary", "nnue-tool", "python"):
        prep.add_argument("--" + name, type=Path, required=True)
    prep.add_argument("--parent-samples", type=Path, action="append", required=True)
    prep.add_argument("--revision", required=True)
    prep.add_argument("--lease-index", type=int, default=7)
    prep.add_argument("--trainers", type=int, choices=(1, 2), default=1)
    commands.add_parser("run").add_argument("out", type=Path)
    args = parser.parse_args()
    if args.command == "prepare":
        prepare(args)
    else:
        run(args.out)


if __name__ == "__main__":
    main()
