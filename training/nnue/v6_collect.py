"""Collect frozen v6 evidence on one game-boundary CPU; never trains or admits models."""

import argparse
import fcntl
import json
import os
import shutil
import signal
import sys
import time
from collections import Counter
from pathlib import Path

import v6_data as data

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tactical_tree"))
from supervise import BoundaryLeases, allocation_busy

from run import STOP, Oracle, atomic


def defaults():
    return dict(
        policy=data.POLICY,
        workers=1,
        seed=20261103,
        representative=32,
        challenge=8,
        pilot_roots=1000,
        pilot_challenger=100,
        audit_roots=6000,
        deep_roots=1600,
        branch_roots=2000,
        branch_audit_roots=1000,
        branch_deep_roots=200,
        budgets=dict(broad=10000, audit=30000, deep=120000),
        q_budget_ms=1000,
        request_timeout_seconds=900,
        max_attempts=2,
        minimum_free_bytes=4 * 1024**3,
    )


def freeze_model(source, out, name):
    source = source.resolve(strict=True)
    manifest = json.loads(source.read_text())
    target = out / "models" / name
    target.mkdir(parents=True, exist_ok=True)
    network = manifest.get("weights", {}).get("nnue")
    identities = {}
    if network:
        blob = (source.parent / network["file"]).resolve(strict=True)
        if data.file_hash(blob) != network["sha256"]:
            raise ValueError("NNUE checksum mismatch: " + str(blob))
        dest = target / "network.nnue"
        if not dest.exists():
            shutil.copyfile(blob, dest)
        if data.file_hash(dest) != network["sha256"]:
            raise ValueError("frozen NNUE checksum mismatch")
        network["file"] = dest.name
        identities[str(dest)] = network["sha256"]
    path = target / "model.json"
    atomic(path, manifest)
    identities[str(path)] = data.file_hash(path)
    return dict(
        path=str(path),
        original=str(source),
        original_sha256=data.file_hash(source),
        sha256=identities[str(path)],
        network_sha256=network["sha256"] if network else None,
    ), identities


def prepare(args):
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=True)
    if (out / "config.json").exists():
        raise ValueError("configuration exists; run it or choose a fresh output directory")
    config = defaults()
    control = args.run.resolve() / "analysis"
    coordinator = json.loads((control / "config.json").read_text())
    cpus = coordinator["cpus"]
    if args.lease_index in (0, 2) or args.lease_index not in range(len(cpus)):
        raise ValueError("choose a flexible tournament worker index")
    # Preparation itself is bounded to the same one CPU; no live configuration changes.
    os.sched_setaffinity(0, {cpus[args.lease_index]})
    snapshot = out / "source-state.json"
    shutil.copyfile(args.run / "state.json", snapshot)
    models = {}
    identities = {str(snapshot): data.file_hash(snapshot)}
    for name in ("primary", "challenger", "diagnostic"):
        source = getattr(args, name)
        if source:
            models[name], pinned = freeze_model(source, out, name)
            identities.update(pinned)
    binary = args.binary.resolve(strict=True)
    if not os.access(binary, os.X_OK):
        raise ValueError("worker is not executable")
    # The collection code is an immutable deployment, including shared dependencies.
    code = [
        *Path(__file__).parent.glob("*.py"),
        *Path(__file__).resolve().parents[1].joinpath("tactical_tree").glob("*.py"),
    ]
    identities.update({str(path.resolve()): data.file_hash(path) for path in [binary, *code]})
    config.update(
        out=str(out),
        source_repo=str(args.source_repo.resolve()),
        source_run=str(args.run.resolve()),
        control=str(control),
        lease_index=args.lease_index,
        cpus=cpus,
        binary=str(binary),
        state_sha256=identities[str(snapshot)],
        models=models,
        primary="primary",
        challenger="challenger",
        diagnostic="diagnostic" if args.diagnostic else None,
        identities=identities,
        parent_samples=[str(path.resolve()) for path in args.parent_samples],
        revision=args.revision,
        current_rules_confirmed=True,
        created=time.time(),
    )
    data.validate_config(config)
    atomic(out / "config.json", config)
    print(json.dumps(dict(config=str(out / "config.json"), state_sha256=config["state_sha256"], workers=1)))


def accepted_label(events, final):
    """Only a completed root iteration; never a partial-root replacement or a q timeout."""
    iterations = [event for event in events if event.get("event") == "iteration" and event.get("depth", 0) > 0]
    if not iterations:
        if final.get("terminal") is not None:
            return None
        raise ValueError("no completed search iteration; no label accepted")
    label = max(iterations, key=lambda event: event["depth"])
    if label["depth"] > final.get("completed_depth", 0):
        raise ValueError("completed-depth telemetry mismatch")
    return dict(
        score_black=label["score_black"],
        completed_depth=label["depth"],
        best=label["best"],
        nodes=label["nodes"],
        candidates=label.get("candidates", []),
        bound="completed_root_search",
        mate_claim=abs(label["score_black"]) >= 900000,
        mate_verified=False,
        score_perspective="black-absolute",
        search_is_selective=True,
        candidate_scores_are_heuristic=True,
    )


def choose_alternatives(best, candidates):
    chosen = []

    def add(move):
        if move is not None and not any(move == old for old in chosen):
            chosen.append(move)

    valid = [row["move"] for row in candidates]
    if best in valid:
        add(best)
    if candidates:
        add(max(candidates, key=lambda row: row["static_root"])["move"])
    for row in candidates:
        if len(chosen) >= 3:
            break
        add(row["move"])
    return chosen


def plan_children(db, config, position, moves):
    # Root subsets are frozen by deterministic ordering, not completion timing.
    roots = [
        row[0] for row in db.execute("SELECT position_id FROM jobs WHERE tier='alternatives' ORDER BY position_id")
    ]
    ordinal = roots.index(position["id"])
    for index, move in enumerate(moves):
        payload = dict(
            position,
            path=[move],
            parent=position["id"],
            stratum="alternative",
            selection="verified_legal_sibling",
            saved=None,
            outcome=None,
            outcome_eligible=False,
            branch_index=index,
        )
        payload.pop("id", None)
        key = data.add_position(db, payload)
        data.add_job(db, key, config["primary"], "child", config["budgets"]["broad"], 31)
        if index < 2 and ordinal < config["branch_audit_roots"]:
            for teacher in (config["primary"], config["challenger"]):
                data.add_job(db, key, teacher, "child_audit", config["budgets"]["audit"], 32)
        if index < 2 and ordinal < config["branch_deep_roots"]:
            for teacher in (config["primary"], config["challenger"]):
                data.add_job(db, key, teacher, "child_deep", config["budgets"]["deep"], 41)


def cache_probe(db, config, job, position, oracle):
    for row in db.execute(
        "SELECT result FROM jobs WHERE position_id=? AND teacher=? AND status='completed'",
        (position["id"], job["teacher"]),
    ):
        previous = json.loads(row[0])
        if previous.get("probe"):
            return previous["probe"]
    probe = oracle.call(
        dict(
            op="collection_probe",
            root_ply=position["ply"],
            path=position["path"],
            depth=2,
            time_ms=config["q_budget_ms"],
        ),
        lambda event: None,
    )
    # Holdout exposure is checked against both parent lineages. No relabeling makes it fresh.
    exposed = db.execute("SELECT 1 FROM parent_features WHERE hash=?", (probe["canonical_position_hash"],)).fetchone()
    eligible = position["split"] != "train" and not position["inherited"] and not exposed
    if job["teacher"] == config["primary"]:
        db.execute(
            "INSERT OR REPLACE INTO feature_exposure VALUES(?,?,?)",
            (probe["canonical_position_hash"], position["id"], position["split"]),
        )
        splits = {
            row[0]
            for row in db.execute(
                "SELECT DISTINCT split FROM feature_exposure WHERE hash=?", (probe["canonical_position_hash"],)
            )
        }
        if len(splits) > 1:
            eligible = False
            db.execute(
                "UPDATE positions SET holdout_eligible=0 WHERE id IN "
                "(SELECT position_id FROM feature_exposure WHERE hash=?)",
                (probe["canonical_position_hash"],),
            )
        db.execute(
            "UPDATE positions SET probe=?,holdout_eligible=? WHERE id=?",
            (data.encoded(probe), eligible, position["id"]),
        )
    return probe


def execute(db, config, job, position, oracle):
    if job["tier"] == "alternatives":
        source = db.execute(
            "SELECT result FROM jobs WHERE position_id=? AND teacher=? AND tier='broad' AND status='completed'",
            (position["id"], config["primary"]),
        ).fetchone()
        label = json.loads(source[0])["label"]
        result = oracle.call(
            dict(
                op="rank_candidates",
                root_ply=position["ply"],
                candidates=[label["best"], *[row["move"] for row in label["candidates"]]],
                path=[],
            ),
            lambda event: None,
        )
        moves = choose_alternatives(label["best"], result["candidates"])
        plan_children(db, config, position, moves)
        return dict(moves=moves, static_candidates=result["candidates"])
    probe = cache_probe(db, config, job, position, oracle)
    if job["tier"] == "outcome":
        game = json.loads(db.execute("SELECT payload FROM games WHERE id=?", (position["game_id"],)).fetchone()[0])
        expected = {"BlackWins": 1, "WhiteWins": -1, "Draw": 0}[game["result"]]
        game.update(
            outcome_eligible=probe["terminal"] == expected,
            outcome_reason="verified_terminal" if probe["terminal"] == expected else "not_verified_terminal",
        )
        db.execute("UPDATE games SET payload=? WHERE id=?", (data.encoded(game), game["id"]))
        return dict(probe=probe, label=None, outcome_eligible=game["outcome_eligible"])
    if probe["terminal"] is not None:
        return dict(probe=probe, label=None, terminal=probe["terminal"])
    events = []

    def receive(event):
        if event["event"] == "iteration":
            events.append(event)
            db.execute(
                "INSERT OR REPLACE INTO iterations VALUES(?,?,?)", (job["id"], event["depth"], data.encoded(event))
            )
            db.commit()

    final = oracle.call(
        dict(
            op="search",
            root_ply=position["ply"],
            path=position["path"],
            depth=64,
            time_ms=job["budget_ms"],
            quiescence_depth=2,
            collection_policy=True,
        ),
        receive,
    )
    label = accepted_label(events, final)
    return dict(
        probe=probe,
        label=label,
        final=final,
        search_policy=oracle.ready["collection_policy"],
        teacher=config["models"][job["teacher"]],
        worker_sha256=config["identities"][config["binary"]],
        source_state_sha256=config["state_sha256"],
        budget_ms=job["budget_ms"],
        q_budget_ms=config["q_budget_ms"],
        termination="deadline" if final.get("aborted") else "completed_or_iteration_budget_guard",
        budget_scope="search only; replay, static and standalone quiescence separate",
    )


def pilot_summary(db, config):
    if existing := data.get_meta(db, "pilot_summary"):
        if not existing["passed"]:
            raise ValueError("pilot evidence gate failed; inspect pilot-summary.json")
        return
    pending = db.execute("SELECT count(*) FROM jobs WHERE priority<10 AND status IN ('pending','running')").fetchone()[
        0
    ]
    if pending:
        return
    rows = [json.loads(row[0]) for row in db.execute("SELECT result FROM jobs WHERE priority=0 AND status='completed'")]
    labels = [row["label"] for row in rows if row.get("label")]
    summary = dict(
        completed=len(rows),
        usable_labels=len(labels),
        depths=dict(Counter(label["completed_depth"] for label in labels)),
        failed=db.execute("SELECT count(*) FROM jobs WHERE priority<10 AND status='failed'").fetchone()[0],
        q_timeouts=sum(row.get("probe", {}).get("quiescence", {}).get("aborted", False) for row in rows),
        limitations="Collection evidence only; no strength claim or automatic teacher promotion.",
    )
    summary["passed"] = bool(labels) and summary["failed"] <= max(5, len(rows) // 10)
    data.set_meta(db, "pilot_summary", summary)
    db.commit()
    atomic(Path(config["out"]) / "pilot-summary.json", summary)
    if not summary["passed"]:
        raise ValueError("pilot did not produce enough usable evidence; inspect pilot-summary.json")


def status(db, config, state, **extra):
    counts = {row[0]: row[1] for row in db.execute("SELECT status,count(*) FROM jobs GROUP BY status")}
    report = dict(
        state=state,
        updated=time.time(),
        pid=os.getpid(),
        workers=1,
        cpu=config["cpus"][config["lease_index"]],
        jobs=counts,
        stages={key: data.get_meta(db, key) for key in ("initialized", "audit_planned", "deep_planned")},
        **extra,
    )
    atomic(Path(config["out"]) / "status.json", report)
    return report


def run(config_path):
    config = json.loads(config_path.read_text())
    data.validate_config(config)
    out = Path(config["out"])
    cpu = config["cpus"][config["lease_index"]]
    os.sched_setaffinity(0, {cpu})
    for name in ("OMP_NUM_THREADS", "OPENBLAS_NUM_THREADS", "MKL_NUM_THREADS"):
        os.environ[name] = "1"
    for name in list(os.environ):
        if name.startswith("TAIKYOKU_AB_"):
            del os.environ[name]
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: STOP.set())
    with (out / "collector.lock").open("a+") as own:
        fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
        db = data.connect(out)
        lease = None
        waiters = None
        oracle = None
        binding = None
        try:
            status(db, config, "waiting_for_cpu")
            # Queue on one flexible worker lock. Existing game is allowed to finish.
            while not STOP.is_set():
                if lease is None:
                    if waiters is None:
                        waiters = BoundaryLeases(Path(config["control"]), [config["lease_index"]], 1)
                    acquired = waiters.take()
                    if acquired is None:
                        STOP.wait(1)
                        continue
                    _, lease = acquired
                    waiters.close()
                    waiters = None
                    os.environ["TACTICAL_LEASE_FD"] = str(lease.fileno())
                if allocation_busy(Path(config["control"])):
                    if oracle:
                        oracle.close()
                        oracle = None
                        binding = None
                    lease.close()
                    lease = None
                    status(db, config, "yielding_to_maintenance")
                    STOP.wait(1)
                    continue
                if shutil.disk_usage(out).free < config["minimum_free_bytes"]:
                    raise ValueError("storage headroom exhausted; collection stopped")
                if not data.get_meta(db, "initialized"):
                    status(db, config, "preparing_snapshot")
                    data.set_meta(db, "config_sha256", data.file_hash(config_path))
                    data.initialize(db, config)
                    db.commit()
                elif data.get_meta(db, "config_sha256") != data.file_hash(config_path):
                    raise ValueError("collection configuration changed")
                # An interrupted request is retried; completed rows are immutable.
                db.execute("UPDATE jobs SET status='pending' WHERE status='running'")
                db.commit()
                pilot_summary(db, config)
                job = db.execute(
                    "SELECT j.* FROM jobs j JOIN positions p ON p.id=j.position_id "
                    "WHERE j.status='pending' ORDER BY j.priority,j.teacher,p.game_id,p.ply,j.id LIMIT 1"
                ).fetchone()
                if job is None:
                    if not data.get_meta(db, "audit_planned"):
                        data.plan_audits(db, config)
                        continue
                    if not data.get_meta(db, "deep_planned"):
                        data.plan_deep(db, config)
                        continue
                    counts = status(db, config, "completed")
                    if counts["jobs"].get("failed"):
                        status(db, config, "finished_with_failures")
                        raise ValueError("collection has failed jobs; inspect catalogue errors")
                    break
                position = json.loads(
                    db.execute("SELECT payload FROM positions WHERE id=?", (job["position_id"],)).fetchone()[0]
                )
                game = json.loads(
                    db.execute("SELECT payload FROM games WHERE id=?", (position["game_id"],)).fetchone()[0]
                )
                key = (game["id"], job["teacher"])
                if binding != key:
                    if oracle:
                        oracle.close()
                        oracle = None
                    if data.file_hash(game["path"]) != game["id"]:
                        raise ValueError("frozen game changed")
                    oracle = Oracle(
                        dict(config, environment=dict(os.environ)),
                        dict(game=game["path"], start_ply=1),
                        out,
                        config["models"][job["teacher"]]["path"],
                    )
                    if oracle.ready.get("collection_protocol") != 1:
                        raise ValueError("worker lacks v6 collection protocol")
                    binding = key
                status(
                    db,
                    config,
                    "collecting",
                    active=dict(
                        job=job["id"], tier=job["tier"], teacher=job["teacher"], game=game["id"], ply=position["ply"]
                    ),
                )
                db.execute("UPDATE jobs SET status='running',attempts=attempts+1 WHERE id=?", (job["id"],))
                db.commit()
                try:
                    result = execute(db, config, job, position, oracle)
                    db.execute(
                        "UPDATE jobs SET status='completed',result=?,error=NULL WHERE id=?",
                        (data.encoded(result), job["id"]),
                    )
                    db.commit()
                except InterruptedError:
                    db.rollback()
                    db.execute("UPDATE jobs SET status='pending',attempts=attempts-1 WHERE id=?", (job["id"],))
                    db.commit()
                    break
                except Exception as error:
                    db.rollback()
                    state = "failed" if job["attempts"] + 1 >= config["max_attempts"] else "pending"
                    db.execute("UPDATE jobs SET status=?,error=? WHERE id=?", (state, str(error), job["id"]))
                    db.commit()
                    oracle.close()
                    oracle = None
                    binding = None
                    print(f"job {job['id']} {state}: {error}", file=sys.stderr, flush=True)
            if STOP.is_set():
                status(db, config, "stopped")
        except Exception as error:
            status(db, config, "failed", error=str(error))
            raise
        finally:
            if oracle:
                oracle.close()
            if waiters:
                waiters.close()
            if lease:
                lease.close()
            db.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    for name in ("out", "run", "source-repo", "binary", "primary", "challenger"):
        prepare_parser.add_argument("--" + name, type=Path, required=True)
    prepare_parser.add_argument("--diagnostic", type=Path)
    prepare_parser.add_argument("--parent-samples", type=Path, action="append", required=True)
    prepare_parser.add_argument("--lease-index", type=int, default=7)
    prepare_parser.add_argument("--revision", required=True)
    commands.add_parser("run").add_argument("config", type=Path)
    commands.add_parser("status").add_argument("config", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(args)
        elif args.command == "run":
            run(args.config)
        else:
            config = json.loads(args.config.read_text())
            print((Path(config["out"]) / "status.json").read_text())
    except (OSError, ValueError, RuntimeError) as error:
        parser.exit(1, f"v6 collection: {error}; collector did not complete successfully\n")


if __name__ == "__main__":
    main()
