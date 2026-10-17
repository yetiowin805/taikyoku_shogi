"""Run or resume an isolated, budgeted tactical-tree experiment (no training-data writes)."""

import argparse
import hashlib
import json
import os
import queue
import signal
import subprocess
import threading
import time
from pathlib import Path

from tree import Tree

STOP = threading.Event()


def atomic(path, data):
    path = Path(path)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w") as f:
        json.dump(data, f, separators=(",", ":"))
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def sha(path):
    with Path(path).open("rb") as f:
        digest = hashlib.sha256()
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
        return digest.hexdigest()


class Oracle:
    def __init__(self, config, case, out, model=None):
        self.events = queue.Queue()
        self.log = (out / "engine.log").open("ab")
        self.proc = subprocess.Popen(
            [config["binary"], case["game"], str(case["start_ply"]), model or config["model"]],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=self.log,
            text=True,
            start_new_session=True,
            pass_fds=([int(os.environ["TACTICAL_LEASE_FD"])] if "TACTICAL_LEASE_FD" in os.environ else []),
        )

        def read():
            try:
                for line in self.proc.stdout:
                    self.events.put(json.loads(line))
            except Exception as exc:
                self.events.put(dict(event="error", error=str(exc)))
            finally:
                self.events.put(dict(event="eof"))

        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()
        try:
            self.ready = self.events.get(timeout=120)
        except queue.Empty:
            self.close()
            raise RuntimeError("oracle startup did not produce a ready event") from None
        if self.ready.get("event") != "ready":
            self.close()
            raise RuntimeError(f"oracle startup failed: {self.ready}")

    def call(self, request, receive):
        request = dict(version=1, id=str(time.time_ns()), **request)
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()
        # Search budgets are intentional experimental actions. A wedged helper must not consume the whole pilot.
        deadline = time.monotonic() + max(30, request.get("time_ms", 0) / 1000 * 2 + 10)
        while True:
            if STOP.is_set():
                raise InterruptedError("pilot paused")
            if time.monotonic() > deadline:
                raise TimeoutError("helper exceeded its per-action safety margin; completed iterations retained")
            try:
                event = self.events.get(timeout=0.2)
            except queue.Empty:
                continue
            if event["event"] in ("error", "eof"):
                raise RuntimeError(str(event))
            if event.get("id") != request["id"]:
                raise RuntimeError("oracle response ID mismatch")
            receive(event)
            if event["event"] == "complete":
                return event

    def close(self):
        if self.proc.poll() is None:
            os.killpg(self.proc.pid, signal.SIGTERM)
            try:
                self.proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL)
                self.proc.wait()
        self.reader.join(timeout=2)
        self.proc.stdin.close()
        self.proc.stdout.close()
        self.log.close()


def run_case(config, case, strategy, budget_s, out):
    out.mkdir(parents=True, exist_ok=True)
    checkpoint = out / "tree.json"
    saved = json.loads(checkpoint.read_text()) if checkpoint.exists() else None
    if saved:
        if saved["identities"] != config["identities"] or saved["strategy"] != strategy or saved["case"] != case:
            raise ValueError("resume identity mismatch")
        if saved.get("finished") and saved["spent"] >= budget_s:
            return
    oracle = Oracle(config, case, out)
    tree = Tree(oracle.ready["actual"], case["end_ply"] - case["start_ply"], saved)
    if saved is None:
        tree.data.update(spent=0, strategy=strategy, case=case, identities=config["identities"], snapshots=[])
    tree.data["finished"] = False
    tree.data["budget_seconds"] = budget_s
    start = time.monotonic()
    prior = tree.data["spent"]
    events = (out / "events.jsonl").open("a", buffering=1)

    def save():
        tree.data["spent"] = prior + time.monotonic() - start
        atomic(checkpoint, tree.data)

    def call(number, op, **extra):
        action_start = time.monotonic()

        def receive(event):
            events.write(json.dumps({"node": number, "op": op, **event}) + "\n")
            events.flush()
            os.fsync(events.fileno())
            if event["event"] == "iteration":
                tree.iteration(number, event)
                save()  # Every completed iteration is durable before the next one starts.

        result = oracle.call(dict(op=op, path=tree.nodes[number]["path"], **extra), receive)
        if op == "inspect":
            tree.inspect(number, result)
        else:
            n = tree.nodes[number]
            n["searches"] += 1
            n["last_cost"] = time.monotonic() - action_start
            n["failed_attempts"] = n["failed_attempts"] + 1 if tree.depth(number) < extra["depth"] else 0
            if result.get("terminal") is not None:
                n["terminal"] = result["terminal"]
        save()

    def search(number, depth, initial=False):
        n = tree.nodes[number]
        if n["terminal"] is not None:
            return
        remaining = budget_s - (prior + time.monotonic() - start)
        if remaining <= 0.1:
            return
        seconds = 3 if initial else min(30, max(3, n["last_cost"] * 2) * 2 ** min(n["failed_attempts"], 4))
        call(number, "search", depth=max(1, depth), time_ms=max(1, int(min(seconds, remaining) * 1000)))

    try:
        for number in tree.spine():
            if prior + time.monotonic() - start >= budget_s or STOP.is_set():
                break
            if tree.nodes[number]["legal"] is None:
                call(number, "inspect")
            if not tree.nodes[number]["records"]:
                search(number, 3, True)
        while prior + time.monotonic() - start < budget_s and not STOP.is_set():
            action = tree.action(strategy)
            if action is None:
                tree.data["fully_expanded"] = True
                break
            number = action["node"]
            tree.data["current_action"] = action
            save()
            if tree.nodes[number]["legal"] is None or action.get("reply"):
                call(number, "inspect", reply=action.get("reply"))
            if action["kind"] == "search":
                search(number, action["depth"])
            tree.data["actions"] += 1
            diagnosis = tree.diagnosis(case["start_ply"])
            elapsed = prior + time.monotonic() - start
            if not tree.data["snapshots"] or elapsed - tree.data["snapshots"][-1]["elapsed"] >= 30:
                tree.data["snapshots"].append(dict(elapsed=elapsed, diagnosis=diagnosis, nodes=len(tree.nodes)))
            tree.data["diagnosis"] = diagnosis
            save()
        tree.data["finished"] = not STOP.is_set()
        tree.data["diagnosis"] = tree.diagnosis(case["start_ply"])
        save()
    finally:
        tree.data["diagnosis"] = tree.diagnosis(case["start_ply"])
        save()
        oracle.close()
        events.close()
        atomic(
            out / "summary.json",
            {k: tree.data[k] for k in ("case", "strategy", "spent", "actions", "finished", "diagnosis", "snapshots")},
        )


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("config", type=Path)
    ap.add_argument("--case", required=True)
    ap.add_argument("--strategy", choices=["local", "pv", "contested", "refutation"], required=True)
    ap.add_argument("--seconds", type=float)
    args = ap.parse_args()
    config = json.loads(args.config.read_text())
    for path, digest in config["identities"].items():
        if sha(path) != digest:
            raise ValueError("immutable input changed: " + path)
    case = next(c for c in config["cases"] if c["id"] == args.case)
    for sig in [signal.SIGINT, signal.SIGTERM]:
        signal.signal(sig, lambda *_: STOP.set())
    run_case(
        config,
        case,
        args.strategy,
        args.seconds or config["case_seconds"],
        Path(config["out"]) / args.case / args.strategy,
    )


if __name__ == "__main__":
    main()
