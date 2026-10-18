"""Borrow up to two game-boundary leases without changing the trainer's allocation demand."""

import argparse
import fcntl
import json
import os
import queue
import random
import shutil
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

from comparison import STRATEGIES
from leader import bind_case

from run import atomic, validate_inputs

STOP = False


def stop(*_):
    global STOP
    STOP = True


def claim(control, candidates, held):
    # Nonblocking allocation lock: admission may deliberately hold it while draining games.
    with (control / "allocation.lock").open("a+") as admission:
        try:
            fcntl.flock(admission, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return None
        for index in candidates:
            if index in held or index in (0, 2):
                continue
            lease = (control / f"cpu-{index}.lock").open("a+")
            try:
                fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                lease.close()
                continue
            return index, lease
    return None


class BoundaryLeases:
    """Queue on game locks so immediate game reclaims cannot repeatedly beat a polling timer."""

    def __init__(self, control, candidates, maximum):
        self.ready = queue.Queue()
        self.stopped = threading.Event()
        self.guard = threading.Lock()
        self.count = 0
        self.maximum = maximum
        self.threads = []
        for index in candidates:
            thread = threading.Thread(target=self.guarded_wait, args=(control, index), daemon=True)
            thread.start()
            self.threads.append(thread)

    def guarded_wait(self, control, index):
        try:
            self.wait(control, index)
        except Exception as exc:
            self.ready.put(exc)

    def wait(self, control, index):
        while not self.stopped.is_set():
            lease = (control / f"cpu-{index}.lock").open("a+")
            fcntl.flock(lease, fcntl.LOCK_EX)  # Existing game continues normally until it closes its lease.
            with self.guard:
                if self.stopped.is_set() or self.count >= self.maximum:
                    lease.close()
                    return
                with (control / "allocation.lock").open("a+") as admission:
                    try:
                        fcntl.flock(admission, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        lease.close()
                    else:
                        self.count += 1
                        self.ready.put((index, lease))
                        return
            self.stopped.wait(0.2)

    def take(self):
        try:
            result = self.ready.get_nowait()
            if isinstance(result, Exception):
                raise RuntimeError("CPU reservation failed; pilot cannot proceed") from result
            return result
        except queue.Empty:
            return None

    def close(self):
        with self.guard:
            self.stopped.set()
            while not self.ready.empty():
                acquired = self.ready.get_nowait()
                if not isinstance(acquired, Exception):
                    acquired[1].close()
        # Any still-blocked daemon exits without doing work when its game finishes;
        # process exit closes its descriptor immediately. Never wait for a live game here.


def active_admission(config):
    path = Path(config["admission_status"])
    if not path.exists():
        return False
    state = json.loads(path.read_text()).get("state")
    return state not in (None, "waiting_for_training", "admitted", "completed", "failed")


def allocation_busy(control):
    with (control / "allocation.lock").open("a+") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
    return False


def run(path):
    config = json.loads(path.read_text())
    out = Path(config["out"])
    out.mkdir(parents=True, exist_ok=True)
    control = Path(config["control"])
    if config["workers"] not in (1, 2) or not set(config["lease_indices"]) <= {4, 5, 6, 7}:
        raise ValueError("pilot only borrows at most two of workers 4–7")
    if len(config["cpus"]) != 8 or len(set(config["cpus"])) != 8 or not set(config["cpus"]) <= os.sched_getaffinity(0):
        raise ValueError("configured CPUs unavailable")
    validate_inputs(config)
    if shutil.disk_usage(out).free < config.get("minimum_free_bytes", 4 * 1024**3):
        raise ValueError("insufficient storage headroom; experiment did not start")
    own = (out / "supervisor.lock").open("a+")
    fcntl.flock(own, fcntl.LOCK_EX | fcntl.LOCK_NB)
    # Finish comparisons early even if only one game CPU becomes available.
    rng = random.Random(config["seed"])
    cases = [c["id"] for c in config["cases"]]
    rng.shuffle(cases)
    jobs = []
    comparison = config.get("experiment") == "matched-analysis-v1"
    for case in cases:
        variants = list(STRATEGIES) if comparison else ["local", "pv", "contested", "refutation"]
        rng.shuffle(variants)
        jobs.extend((case, strategy) for strategy in variants)
        jobs.append((case, "audit"))
    leases = {}
    running = {}
    finished = []
    failures = []
    started = time.time()
    compute_started = None
    pause_reason = None
    waiters = BoundaryLeases(control, config["lease_indices"], config["workers"])

    def complete(job):
        p = out / job[0] / job[1] / ("audit.json" if job[1] == "audit" else "summary.json")
        field = "work_complete" if job[1] == "audit" else "finished"
        return p.exists() and json.loads(p.read_text()).get(field, False)

    try:
        while jobs or running:
            for index, (process, job, log) in list(running.items()):
                if process.poll() is None:
                    continue
                log.close()
                del running[index]
                if process.returncode or not complete(job):
                    failures.append(dict(job=job, returncode=process.returncode))
                else:
                    finished.append(job)
            yielding = active_admission(config) or allocation_busy(control)
            if shutil.disk_usage(out).free < config.get("minimum_free_bytes", 4 * 1024**3):
                pause_reason = "storage_headroom"
            if STOP or pause_reason:
                for p, _, _ in running.values():
                    p.terminate()
                if not running:
                    break
            if yielding:
                # Maintenance gets the CPU after the current finite job finishes; do not kill work.
                if waiters is not None:
                    waiters.close()
                    waiters = None
                for index in list(leases):
                    if index not in running:
                        leases.pop(index).close()
            if not STOP and not yielding and not pause_reason:
                if len(leases) < config["workers"]:
                    if waiters is None:
                        waiters = BoundaryLeases(
                            control,
                            [i for i in config["lease_indices"] if i not in leases],
                            config["workers"] - len(leases),
                        )
                    acquired = waiters.take()
                    if acquired:
                        index, lease = acquired
                        leases[index] = lease
                        if compute_started is None:
                            compute_started = time.time()
                for index, lease in leases.items():
                    if index in running:
                        continue
                    while jobs and complete(jobs[0]):
                        finished.append(jobs.pop(0))
                    if not jobs:
                        continue
                    # Skip an audit awaiting its final arm instead of idling the other CPU.
                    eligible = next(
                        (
                            i
                            for i, candidate in enumerate(jobs)
                            if candidate[1] != "audit" or not any(j[0] == candidate[0] for _, j, _ in running.values())
                        ),
                        None,
                    )
                    if eligible is None:
                        continue
                    job = jobs.pop(eligible)
                    if complete(job):
                        finished.append(job)
                        continue
                    if comparison:
                        script = "comparison_audit.py" if job[1] == "audit" else "comparison.py"
                        case = next(c for c in config["cases"] if c["id"] == job[0])
                        job_config = bind_case(config, case)
                    else:
                        script = "audit.py" if job[1] == "audit" else "run.py"
                        job_config = path
                    cmd = [
                        "taskset",
                        "-c",
                        str(config["cpus"][index]),
                        sys.executable,
                        str(Path(__file__).with_name(script)),
                        str(job_config),
                        "--case",
                        job[0],
                    ]
                    if job[1] != "audit":
                        cmd += ["--strategy", job[1]]
                    log = (out / f"{job[0]}-{job[1]}.log").open("ab")
                    env = dict(os.environ, TACTICAL_LEASE_FD=str(lease.fileno()))
                    process = subprocess.Popen(cmd, stdout=log, stderr=log, env=env, pass_fds=[lease.fileno()])
                    running[index] = (process, job, log)
            atomic(
                out / "status.json",
                dict(
                    state="yielding_for_maintenance"
                    if yielding
                    else "stopping"
                    if STOP
                    else "running"
                    if running
                    else "waiting_for_cpu",
                    pid=os.getpid(),
                    started=started,
                    updated=time.time(),
                    leased=list(leases),
                    running={str(i): dict(pid=p.pid, case=j[0], strategy=j[1]) for i, (p, j, _) in running.items()},
                    completed=len(finished),
                    remaining=len(jobs),
                    failures=failures,
                    compute_started=compute_started,
                    pause_reason=pause_reason,
                ),
            )
            time.sleep(0.2 if len(leases) < config["workers"] else 1)
        atomic(
            out / "status.json",
            dict(
                state="paused"
                if STOP or yielding or pause_reason
                else "completed_with_failures"
                if failures
                else "completed",
                updated=time.time(),
                completed=len(finished),
                remaining=len(jobs),
                failures=failures,
                leased=[],
                pause_reason=pause_reason,
            ),
        )
        if failures and not STOP and not yielding and not pause_reason:
            raise RuntimeError(f"{len(failures)} pilot jobs failed; see status and individual logs")
    finally:
        if waiters is not None:
            waiters.close()
        for p, _, log in running.values():
            p.terminate()
            p.wait()  # Child performs cooperative cancellation and persists completed iterations.
            log.close()
        for lease in leases.values():
            lease.close()
        own.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    options = parser.parse_args()
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, stop)
    run(options.config.resolve())
