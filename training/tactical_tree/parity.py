"""Completed-depth parity survey; isolated probes never change production settings."""

import argparse
import json
import os
import signal
import time
from pathlib import Path

from run import STOP, Oracle, atomic, sha, validate_inputs


def finite_metrics(records, turn):
    scores = {int(d): r["score_black"] for d, r in records.items()}
    finite = {d: s for d, s in scores.items() if abs(s) < 900000}
    adjacent = {str(d): finite[d] - finite[d - 1] for d in finite if d - 1 in finite}
    same = {str(d): finite[d] - finite[d - 2] for d in finite if d - 2 in finite}
    # A local odd/even offset with a linear depth trend removed: d3 vs mean(d2,d4).
    curvature = finite[3] - (finite[2] + finite[4]) / 2 if all(d in finite for d in (2, 3, 4)) else None
    return dict(
        scores_black=scores,
        adjacent_black=adjacent,
        same_parity_black=same,
        odd_even_offset_black=curvature,
        odd_even_offset_root=None if curvature is None else curvature * turn,
        mate_depths=[d for d in scores if d not in finite],
        changed_moves={
            str(d): records[str(d)]["best"] != records[str(d - 1)]["best"] for d in scores if d - 1 in scores
        },
    )


def run(config, case, model_id):
    out = Path(config["out"]) / case["id"] / model_id
    out.mkdir(parents=True, exist_ok=True)
    path = out / "summary.json"
    model = config["models"][model_id]
    identity = dict(case=case, model=model, binary_sha256=config["binary_sha256"], protocol="parity-v1")
    if path.exists():
        data = json.loads(path.read_text())
        if data["identity"] != identity:
            raise ValueError("parity resume identity mismatch")
        if data.get("finished"):
            return
    else:
        data = dict(identity=identity, finished=False, requests={}, records={}, started=time.time())
    for p, digest in model["identities"].items():
        if sha(p) != digest:
            raise ValueError("model identity changed: " + p)
    if sha(case["game"]) != case["sha256"]:
        raise ValueError("game identity changed")
    # Checkpoint defaults must not be silently replaced by a shell's search overrides.
    for name in list(os.environ):
        if name.startswith(("TAIKYOKU_AB_", "TAIKYOKU_EXPERIMENT_")):
            del os.environ[name]
    oracle = Oracle(config, case, out, model=model["path"])
    log = (out / "events.jsonl").open("a", buffering=1)

    def save():
        data["updated"] = time.time()
        atomic(path, data)

    def request(key, **payload):
        row = data["requests"].setdefault(key, dict(request=payload, iterations={}))
        if row["request"] != payload:
            raise ValueError("request changed on resume")
        if row.get("complete"):
            return row["complete"]

        def receive(event):
            log.write(json.dumps(dict(key=key, **event)) + "\n")
            log.flush()
            os.fsync(log.fileno())
            if event["event"] == "iteration":
                row["iterations"][str(event["depth"])] = event
                save()

        row["complete"] = oracle.call(payload, receive)
        save()
        return row["complete"]

    try:
        if "ready" in data and data["ready"] != oracle.ready:
            raise ValueError("replayed state/search configuration changed")
        data["ready"] = oracle.ready
        root = request("root_eval", op="evaluate", path=[])
        request("root_q2", op="quiescence_probe", path=[], depth=2)
        for depth in range(1, case["maximum_depth"] + 1):
            key = f"root_d{depth}"
            result = request(key, op="search", path=[], depth=depth, fixed_depth=True)
            if result.get("terminal") is not None:
                data["terminal"] = result["terminal"]
                break
            if result["aborted"] or result["completed_depth"] != depth:
                raise ValueError("fixed-depth request did not finish its requested depth")
            record = data["requests"][key]["iterations"][str(depth)]
            data["records"][str(depth)] = dict(record, q_nodes=result["q_nodes"], result=result)
            save()
        # These are legal first continuations, not claimed to be actual quiescence leaves.
        choices = []
        for depth in (3, 4):
            record = data["records"].get(str(depth))
            if record and record["best"] not in choices:
                choices.append(record["best"])
        for i, move in enumerate(choices):
            request(f"child{i}_eval", op="evaluate", path=[move])
            request(f"child{i}_q2", op="quiescence_probe", path=[move], depth=2)
        data["metrics"] = finite_metrics(data["records"], root["turn"])
        data["metrics"]["same_board_turn_gap_black"] = root["static_black"] - root["flipped_static_black"]
        data["finished"] = True
        save()
    finally:
        save()
        oracle.close()
        log.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--case", required=True)
    parser.add_argument("--model", required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    validate_inputs(config)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: STOP.set())
    run(config, next(c for c in config["cases"] if c["id"] == args.case), args.model)
