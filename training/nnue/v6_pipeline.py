"""Durable collection-to-training handoff; isolated candidates, no tournament admission."""

import argparse
import fcntl
import json
import os
import signal
import subprocess
import time
from pathlib import Path

import v6_data as data
import v6_dataset as dataset
from v6_collect import BoundaryLeases, allocation_busy, atomic
from v6_runtime import LeasePool

STOP = False


def stop(*_):
    global STOP
    STOP = True


def configured_widths(config):
    parents = config["parents"]
    if not parents or any(width not in {"256", "384", "512"} for width in parents):
        raise ValueError("parents must select widths from 256, 384, and 512")
    return sorted(int(width) for width in parents)


def tasks(config):
    if config.get("full_v7"):
        from v7_full import tasks as full_tasks

        return full_tasks(config)
    if config.get("sensitivity"):
        from v7_pilot import tasks as pilot_tasks

        return pilot_tasks(config)
    out = Path(config["out"])
    result = []
    widths = configured_widths(config)
    for width in widths:
        for regime in "AB":
            name = f"prelude-{width}-{regime}"
            result.append(
                dict(
                    name=name,
                    dependencies=[],
                    fit=dict(width=width, regime=regime, prelude=True, parent=config["parents"][str(width)]),
                )
            )
        for regime in "ABCD":
            start = f"prelude-{width}-" + ("A" if regime == "A" else "B")
            name = f"NNUE_W{width}_v6{regime}"
            result.append(
                dict(
                    name=name,
                    dependencies=[start],
                    fit=dict(
                        width=width,
                        regime=regime,
                        parent=config["parents"][str(width)],
                        start=str(out / start / "start.pt"),
                    ),
                )
            )
    # Explicit campaign selection preserves old recipes/checkpoints while allowing
    # pure-NNUE experiments to be replaced by distinct material-retaining runs.
    selected = config.get("training_models")
    if selected is None:
        return result
    if not selected or len(selected) != len(set(selected)):
        raise ValueError("training_models must be nonempty and distinct")
    for width in widths:
        for regime in "CD":
            start = f"prelude-{width}-A"
            result.append(
                dict(
                    name=f"NNUE_W{width}_v6{regime}m",
                    dependencies=[start],
                    fit=dict(
                        width=width,
                        regime=regime,
                        retain_material=True,
                        parent=config["parents"][str(width)],
                        start=str(out / start / "start.pt"),
                    ),
                )
            )
    by_name = {job["name"]: job for job in result}
    plan = []
    for name in selected:
        if name not in by_name or by_name[name]["fit"].get("prelude"):
            raise ValueError("unknown training model: " + name)
        for dependency in by_name[name]["dependencies"]:
            if by_name[dependency] not in plan:
                plan.append(by_name[dependency])
        plan.append(by_name[name])
    return plan


def finished(out, job):
    marker = out / job["name"] / "complete.json"
    if not marker.exists():
        return False
    if job["fit"].get("full_v7"):
        storage = marker.with_name("storage-complete.json")
        if not storage.exists():
            return False  # Resume the archival step after an interrupted upload.
        if json.loads(storage.read_text())["complete_sha256"] != data.file_hash(marker):
            raise ValueError("storage completion identity mismatch: " + job["name"])
    cp = out / job["name"] / ("start.pt" if job["fit"].get("prelude") else "model.json")
    if not cp.exists():
        raise ValueError("completion marker without artifact: " + job["name"])
    if cp.suffix == ".json":
        d = json.loads(cp.read_text())["weights"]["nnue"]
        if data.file_hash(cp.parent / d["file"]) != d["sha256"]:
            raise ValueError("completed model changed")
        expected = (
            "residual" if job["fit"]["regime"] in ("A", "Ao", "E") or job["fit"].get("retain_material") else "total"
        )
        if d.get("output_mode") != expected:
            raise ValueError("completed model output mode differs")
    return True


def preflight(config):
    configured_widths(config)
    if config["lease_indices"] != [7, 1, 3, 4, 5, 6]:
        raise ValueError("unexpected CPU lease set")
    for path, digest in config["identities"].items():
        if data.file_hash(path) != digest:
            raise ValueError("pinned input changed: " + path)
    parents = [json.loads(Path(p).read_text()) for p in config["parents"].values()]
    if any(parent["weights"]["piece"] != parents[0]["weights"]["piece"] for parent in parents[1:]):
        raise ValueError("parents have different material baselines")
    for width, p in config["parents"].items():
        d = json.loads(Path(p).read_text())["weights"]["nnue"]
        if d["width"] != int(width) or d.get("output_mode", "residual") != "residual":
            raise ValueError("wrong parent")
        if data.file_hash(Path(p).parent / d["file"]) != d["sha256"]:
            raise ValueError("parent network changed")


def run(config):
    global STOP
    STOP = False
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    out = Path(config["out"])
    if (out / "blocked.json").exists():
        raise ValueError("pipeline blocked by a failed prerequisite; inspect blocked.json before retry")
    preflight(config)
    out.mkdir(parents=True, exist_ok=True)
    pinned = out / "pipeline-config.json"
    if pinned.exists() and json.loads(pinned.read_text()) != config:
        raise ValueError("pipeline configuration changed on resume")
    atomic(pinned, config)
    with (out / "pipeline.lock").open("a+") as own:
        fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
        pool = LeasePool(
            Path(config["control"]),
            BoundaryLeases,
            reserve=config.get("max_parallel", 4) if config.get("sensitivity") else None,
        )
        held = {}
        running = {}
        pausing = set()
        prepared = False
        jobs = tasks(config)
        try:
            while not STOP:
                if (
                    config.get("sensitivity")
                    and not config.get("full_v7")
                    and time.time() >= config["training_deadline"]
                ):
                    atomic(out / "status.json", dict(state="pilot_budget", updated=time.time()))
                    return
                report = dict(
                    updated=time.time(),
                    pid=os.getpid(),
                    cpus=list(held),
                    active=list(running),
                    completed=[j["name"] for j in jobs if finished(out, j)],
                )
                db = dataset.connect(config["catalogue"])
                try:
                    ready = dataset.completed(db)
                finally:
                    db.close()
                # SQLite queue completion alone is insufficient: wait for the collector
                # to commit its final status and release its process lock.
                collection = Path(config["catalogue"]).parent
                status = json.loads((collection / "status.json").read_text())
                ready = ready and status.get("state") in (
                    ("completed", "pilot_budget") if config.get("sensitivity") else ("completed",)
                )
                if not ready:
                    atomic(
                        out / "status.json",
                        dict(report, state="waiting_for_collection", collection_state=status.get("state")),
                    )
                    time.sleep(10)
                    continue
                with (collection / "collector.lock").open("a+") as lock:
                    try:
                        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        time.sleep(1)
                        continue
                maintenance = allocation_busy(Path(config["control"]))
                acquired = pool.update(
                    [i for i in config["lease_indices"] if i not in held]
                    if len(held) < config.get("max_parallel", 6)
                    else [],
                    maintenance,
                )
                if acquired:
                    held[acquired[0]] = acquired[1]
                for name, (proc, _cpu, log) in list(running.items()):
                    code = proc.poll()
                    if code is None:
                        continue
                    log.close()
                    del running[name]
                    if name in pausing:
                        pausing.remove(name)
                        if code not in (0, 75, -signal.SIGTERM):
                            raise RuntimeError(f"{name} failed while pausing: {code}")
                        continue
                    if code != 0:
                        raise RuntimeError(f"{name} failed with exit {code}; see its log")
                    if name == "prepare":
                        dataset.verify(out / "dataset")
                        prepared = True
                    else:
                        job = next(j for j in jobs if j["name"] == name)
                        if not finished(out, job):
                            raise ValueError("trainer exited without completion: " + name)
                        # Full v7 verifies archival before eviction. Legacy campaigns
                        # retain their existing disposable-checkpoint behavior.
                        if not job["fit"].get("full_v7"):
                            (out / name / "training.pt").unlink(missing_ok=True)
                used = {cpu for _, cpu, _ in running.values()}
                if maintenance:
                    for name, (proc, _cpu, _log) in running.items():
                        if name != "prepare" and name not in pausing:
                            proc.terminate()
                            pausing.add(name)
                    for cpu in list(held):
                        if cpu not in used:
                            held.pop(cpu).close()
                if (
                    not prepared
                    and not running
                    and (out / "dataset/dataset.json").exists()
                    and (not config.get("sensitivity") or (out / "views/views.json").exists())
                ):
                    dataset.verify(out / "dataset")
                    prepared = True
                if config.get("sensitivity") and not config.get("full_v7"):
                    from v7_pilot import calibrate_training

                    calibrate_training(config)
                    jobs = tasks(config)
                done = {j["name"] for j in jobs if finished(out, j)}
                if prepared and len(done) == len(jobs):
                    atomic(
                        out / "ready.json",
                        dict(
                            models=[str(out / j["name"] / "model.json") for j in jobs if not j["fit"].get("prelude")],
                            dataset_sha256=data.file_hash(out / "dataset/dataset.json"),
                            promotion="requires playing evaluation",
                        ),
                    )
                    atomic(out / "status.json", dict(report, state="completed"))
                    return
                if not maintenance:
                    for cpu in held:
                        if cpu in used:
                            continue
                        if not prepared:
                            if running:
                                break
                            name = "prepare"
                            args = [
                                config["python"],
                                str(Path(__file__).with_name("v6_dataset.py")),
                                config["config_path"],
                            ]
                        else:
                            candidates = [
                                j
                                for j in jobs
                                if j["name"] not in done and j["name"] not in running and set(j["dependencies"]) <= done
                            ]
                            if not candidates:
                                break
                            job = candidates[0]
                            name = job["name"]
                            fit = dict(
                                job["fit"],
                                dataset=str(out / "dataset"),
                                out=str(out / name),
                                seed=20261103,
                                threads=1,
                                window_draws=config.get("window_draws", 65536),
                            )
                            if config.get("sensitivity"):
                                fit.update(job["fit"])
                            path = out / (name + ".json")
                            atomic(path, fit)
                            args = [config["python"], str(Path(__file__).with_name("v6_fit.py")), str(path)]
                        log = (out / (name + ".log")).open("ab")
                        env = dict(
                            os.environ,
                            OMP_NUM_THREADS="1",
                            MKL_NUM_THREADS="1",
                            OPENBLAS_NUM_THREADS="1",
                            TACTICAL_LEASE_FD=str(held[cpu].fileno()),
                        )
                        proc = subprocess.Popen(
                            ["taskset", "-c", str(cpu), *args],
                            stdout=log,
                            stderr=subprocess.STDOUT,
                            env=env,
                            pass_fds=(held[cpu].fileno(),),
                        )
                        running[name] = (proc, cpu, log)
                        used.add(cpu)
                atomic(
                    out / "status.json",
                    dict(report, state="training" if prepared else "preparing" if running else "waiting_for_cpu"),
                )
                time.sleep(1)
        except Exception as error:
            atomic(out / "blocked.json", dict(updated=time.time(), error=str(error)))
            atomic(out / "status.json", dict(state="failed", updated=time.time(), error=str(error)))
            raise
        finally:
            for proc, _, _log in running.values():
                if proc.poll() is None:
                    proc.terminate()
            for proc, _, log in running.values():
                proc.wait()
                log.close()
            pool.close()
            for lease in held.values():
                lease.close()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("config", type=Path)
    a = p.parse_args()
    config = json.loads(a.config.read_text())
    config["config_path"] = str(a.config.resolve())
    run(config)


if __name__ == "__main__":
    main()
