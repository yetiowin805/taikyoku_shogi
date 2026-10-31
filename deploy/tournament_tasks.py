"""Submit durable commands to the running tournament's single state writer."""

import argparse
import fcntl
import json
import os
import time
from pathlib import Path

import tourney_analysis as a

PROTOCOL = 1


def ready(run):
    control = Path(run) / "analysis/tasks"
    path = control / "ready.json"
    if not path.exists():
        raise ValueError("Coordinator does not support live tasks; upgrade it before admission")
    meta = a.read(path)
    if meta.get("protocol") != PROTOCOL or not a.alive(meta):
        raise ValueError("Live task coordinator is unavailable or incompatible")
    error = control / "error.json"
    if error.exists():
        raise RuntimeError("Coordinator task service failed: " + a.read(error)["error"])
    return control


def durable(path, data):
    """Publish and fsync a request, without ever truncating an existing request."""
    path = Path(path)
    temporary = path.with_suffix(f".{os.getpid()}.{time.time_ns()}.tmp")
    try:
        with temporary.open("xb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    finally:
        temporary.unlink(missing_ok=True)


def submit(run, command, task_id=None):
    run = Path(run)
    control = ready(run)
    request = dict(protocol=PROTOCOL, run_id=a.read(run / "state.json")["run_id"], command=command)
    data = json.dumps(request, sort_keys=True, separators=(",", ":")).encode()
    task_id = task_id or f"{time.time_ns():020d}-{a.digest(data)[:16]}"
    if not task_id or any(c not in "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_" for c in task_id):
        raise ValueError("Task ID must contain only letters, numbers, underscores or hyphens")
    with (control / "submit.lock").open("a+") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        path = control / "requests" / f"{task_id}.json"
        if path.exists():
            if path.read_bytes() != data:
                raise ValueError("Task ID already refers to a different immutable request")
        else:
            durable(path, data)
    return task_id, a.digest(data)


def wait(run, task_id, digest):
    """No elapsed-time cutoff. A dead coordinator or explicit rejection fails loudly."""
    path = Path(run) / "analysis/tasks/receipts" / f"{task_id}.json"
    while True:
        if path.exists():
            result = a.read(path)
            if result.get("sha256") != digest:
                raise ValueError("Task receipt does not match submitted content")
            if result.get("status") != "applied":
                raise RuntimeError("Coordinator rejected task: " + str(result.get("error")))
            return result
        ready(run)
        time.sleep(0.25)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument("action", choices=["pause", "resume", "status"])
    parser.add_argument("--reason", default="operator maintenance")
    args = parser.parse_args()
    if args.action == "status":
        print(json.dumps(a.read(args.run / "state.json").get("runtime_control", {}), indent=2))
        return
    command = (
        dict(kind="pause_assignments", reason=args.reason)
        if args.action == "pause"
        else dict(kind="resume_assignments")
    )
    task_id, digest = submit(args.run, command)
    print(json.dumps(dict(task_id=task_id, receipt=wait(args.run, task_id, digest)), indent=2))


if __name__ == "__main__":
    main()
