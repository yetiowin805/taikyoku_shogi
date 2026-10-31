"""Matched-CPU root search versus branch exploration; checkpoints never kill searches."""

import argparse
import copy
import json
import math
import os
import signal
import time
from pathlib import Path

from tree import Tree

from run import STOP, Oracle, atomic, validate_inputs

STRATEGIES = ("uniform", "adaptive", "branches")


class CpuClock:
    def __init__(self):
        self.start = time.process_time()
        self.pid = None
        self.child = 0.0

    def read(self):
        if self.pid is not None:
            try:
                fields = Path(f"/proc/{self.pid}/stat").read_text().rsplit(") ", 1)[1].split()
                self.child = (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
            except FileNotFoundError:
                pass  # Retain the final observed child time after an exit.
        return time.process_time() - self.start + self.child


def action(tree, strategy):
    if strategy == "branches":
        return tree.action("contested")
    spine = [n for n in tree.spine() if tree.nodes[n]["terminal"] is None]
    if not spine:
        return None
    if strategy == "uniform":
        number = spine[tree.data["cursor"] % len(spine)]
        tree.data["cursor"] += 1
    else:

        def priority(n):
            node = tree.nodes[n]
            scores = [c["score_black"] for c in node["candidates"][:2]]
            gap = abs(scores[0] - scores[1]) if len(scores) == 2 else 1000
            return (400 + min(10000, tree.parity(n)) + 1000000 / (1000 + gap)) / math.sqrt(1 + node["searches"])

        number = max(spine, key=priority)
    return dict(node=number, kind="search", reason=strategy, depth=max(3, tree.depth(number) + 1))


def recommendations(tree, strategy, case):
    spine = tree.spine()
    if strategy == "branches":
        tree.backup()
    rows = []
    for number in spine[:-1]:
        node = tree.nodes[number]
        depth = tree.depth(number)
        direct = node["records"].get(str(depth), {})
        rows.append(
            dict(
                ply=case["start_ply"] + len(node["path"]),
                depth=depth,
                best=node.get("selected") if strategy == "branches" else direct.get("best"),
                score_black=node.get("value") if strategy == "branches" else direct.get("score_black"),
                direct_score_black=direct.get("score_black"),
                parity_spread=tree.parity(number),
            )
        )
    return rows


def observe(data, cpu, rows, checkpoints):
    """Never credit results completed after a checkpoint to that earlier checkpoint."""
    current = dict(cpu_seconds=cpu, recommendations=rows)
    for point in checkpoints:
        if str(point) not in data["snapshots"] and cpu >= point:
            eligible = current if cpu == point else data.get("latest", dict(cpu_seconds=0, recommendations=[]))
            data["snapshots"][str(point)] = dict(copy.deepcopy(eligible), observed_cpu_seconds=cpu)
    data["latest"] = current


def run(config, case, strategy):
    out = Path(config["out"]) / case["id"] / strategy
    out.mkdir(parents=True, exist_ok=True)
    path = out / "tree.json"
    saved = json.loads(path.read_text()) if path.exists() else None
    if saved and (
        saved["identities"] != config["identities"] or saved["strategy"] != strategy or saved["case"] != case
    ):
        raise ValueError("comparison resume identity mismatch")
    if saved and saved.get("finished"):
        return
    clock = CpuClock()
    oracle = Oracle(config, case, out)
    clock.pid = oracle.proc.pid
    tree = Tree(oracle.ready["actual"], case["end_ply"] - case["start_ply"], saved)
    if saved is None:
        tree.data.update(
            spent=0,
            wall_seconds=0,
            snapshots={},
            strategy=strategy,
            case=case,
            identities=config["identities"],
            leader=config.get("leader"),
            resumes=0,
        )
    else:
        tree.data["resumes"] += 1
    data = tree.data
    data["finished"] = False
    prior, wall_prior = data["spent"], data["wall_seconds"]
    started = time.monotonic()
    checkpoints = config["checkpoints"]
    limit = max(checkpoints)
    events = (out / "events.jsonl").open("a", buffering=1)

    def save():
        rows = recommendations(tree, strategy, case)
        data["spent"] = prior + clock.read()
        data["wall_seconds"] = wall_prior + time.monotonic() - started
        observe(data, data["spent"], rows, checkpoints)
        atomic(path, data)

    def call(number, op, **extra):
        before = clock.read()

        def receive(event):
            events.write(json.dumps({"node": number, "op": op, "cpu_seconds": prior + clock.read(), **event}) + "\n")
            events.flush()
            os.fsync(events.fileno())
            # Freeze pre-event evidence if the checkpoint was crossed while the search ran.
            save()
            if event["event"] == "iteration":
                tree.iteration(number, event)
                save()

        result = oracle.call(dict(op=op, path=tree.nodes[number]["path"], **extra), receive)
        node = tree.nodes[number]
        if op == "inspect":
            tree.inspect(number, result)
        else:
            node["searches"] += 1
            node["last_cost"] = clock.read() - before
            node["failed_attempts"] = node["failed_attempts"] + 1 if tree.depth(number) < extra["depth"] else 0
            if result.get("terminal") is not None:
                node["terminal"] = result["terminal"]
        save()

    def search(number, depth, initial=False):
        node = tree.nodes[number]
        if node["terminal"] is not None:
            return
        seconds = 3 if initial else min(30, max(3, node["last_cost"] * 2) * 2 ** min(node["failed_attempts"], 4))
        call(number, "search", depth=depth, time_ms=max(1, int(seconds * 1000)))

    try:
        save()
        for number in tree.spine():
            if prior + clock.read() >= limit or STOP.is_set():
                break
            if tree.nodes[number]["legal"] is None:
                call(number, "inspect")
            if not tree.nodes[number]["records"]:
                search(number, 3, True)
        while prior + clock.read() < limit and not STOP.is_set():
            next_action = action(tree, strategy)
            if next_action is None:
                data["exhausted"] = True
                break
            data["current_action"] = next_action
            number = next_action["node"]
            if tree.nodes[number]["legal"] is None:
                call(number, "inspect")
            if next_action["kind"] == "search":
                search(number, next_action["depth"])
            data["actions"] += 1
            save()
        save()
        if data.get("exhausted"):
            for point in checkpoints:
                data["snapshots"].setdefault(str(point), copy.deepcopy(data["latest"]))
        data["finished"] = not STOP.is_set()
    finally:
        save()
        oracle.close()
        events.close()
        atomic(
            out / "summary.json",
            {
                k: data[k]
                for k in (
                    "case",
                    "strategy",
                    "spent",
                    "wall_seconds",
                    "snapshots",
                    "finished",
                    "leader",
                    "resumes",
                )
            },
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--case", required=True)
    parser.add_argument("--strategy", choices=STRATEGIES, required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    validate_inputs(config)
    case = next(c for c in config["cases"] if c["id"] == args.case)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: STOP.set())
    run(config, case, args.strategy)
