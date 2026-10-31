"""Independent, equal-horizon reply checks of pooled discoveries; estimates, not ground truth."""

import argparse
import json
import signal
import time
from pathlib import Path

from rollout import evaluate as rollout_evaluate
from rollout import summarize as rollout_summarize
from tree import key

from run import STOP, Oracle, atomic, record_runner, validate_inputs


def compare(rows, turn):
    """Compare candidate and played move only at identical completed depths of both parities."""
    played = next((r for r in rows if r.get("played")), None)
    if played is None:
        return []
    comparisons = []
    for row in rows:
        if row is played:
            continue
        depths = set(row["records"]) & set(played["records"])
        odd = [int(d) for d in depths if int(d) % 2]
        even = [int(d) for d in depths if not int(d) % 2]
        use = [max(ds) for ds in (odd, even) if ds]
        gains = [turn * (row["records"][str(d)]["score_black"] - played["records"][str(d)]["score_black"]) for d in use]
        comparisons.append(
            dict(
                move=row["move"],
                depths=use,
                gains=gains,
                robust_gain=min(gains) if len(use) == 2 else None,
                supported_improvement=len(use) == 2 and min(gains) >= 1000,
            )
        )
    return comparisons


def run(config, case):
    out = Path(config["out"]) / case["id"] / "audit"
    out.mkdir(parents=True, exist_ok=True)
    path = out / "audit.json"
    data = (
        json.loads(path.read_text())
        if path.exists()
        else dict(finished=False, spent=0, rows=[], roots=[], case=case, identities=config["identities"])
    )
    if data["identities"] != config["identities"] or data["case"] != case:
        raise ValueError("audit resume identity mismatch")
    if data.get("work_complete"):
        return
    data["finished"] = False
    record_runner(data, config)
    summaries = []
    for strategy in ("local", "pv", "contested", "refutation"):
        p = out.parent / strategy / "summary.json"
        if p.exists():
            summaries.append(json.loads(p.read_text()))
    if len(summaries) != 4:
        raise ValueError("audit needs all four completed strategy summaries")
    votes = {}
    for s in summaries:
        for rank, d in enumerate(s["diagnosis"][:2]):
            votes[d["ply"]] = votes.get(d["ply"], 0) + 2 - rank
    ranked = sorted(votes, key=lambda p: (-votes[p], p))
    positions = list(dict.fromkeys([case["anchor_ply"], *ranked]))[:3]
    oracle = Oracle(config, case, out)
    actual = oracle.ready["actual"]
    started = time.monotonic()
    prior = data["spent"]

    def save():
        data["spent"] = prior + time.monotonic() - started
        atomic(path, data)

    def query(ply, move=None, seconds=30):
        rows = data["roots"] if move is None else data["rows"]
        existing = next((r for r in rows if r["ply"] == ply and r.get("move") == move), None)
        if existing and existing.get("done"):
            return existing
        row = existing or dict(ply=ply, move=move, records={})
        if existing is None:
            rows.append(row)
        branch = actual[: ply - case["start_ply"]] + ([] if move is None else [move])

        def receive(event):
            if event["event"] == "iteration":
                row["records"][str(event["depth"])] = event
                save()

        if STOP.is_set():
            return row
        row["result"] = oracle.call(
            dict(op="search", path=branch, depth=6, time_ms=max(1, int(seconds * 1000))), receive
        )
        row["done"] = True
        save()
        return row

    try:
        for ply in positions:
            query(ply)
        plans = []
        for ply in positions:
            played = actual[ply - case["start_ply"]]
            proposals = {key(played): played}
            for root in data["roots"]:
                if root["ply"] == ply and root["records"]:
                    record = root["records"][str(max(map(int, root["records"])))]
                    if record["best"] is not None:
                        proposals[key(record["best"])] = record["best"]
            for s in summaries:
                diagnosis = next(d for d in s["diagnosis"] if d["ply"] == ply)
                if diagnosis["best"] is not None:
                    proposals[key(diagnosis["best"])] = diagnosis["best"]
            plans.append((ply, played, list(proposals.values())))
        # Round-robin across locations, persisting useful intermediate coverage throughout the audit.
        for i in range(max(map(lambda p: len(p[2]), plans), default=0)):
            for ply, played, moves in plans:
                if i < len(moves):
                    row = query(ply, moves[i], 25)
                    row["played"] = moves[i] == played
                    save()
        data["comparisons"] = []
        first_turn = 1 if case["start_color"] == "Black" else -1
        for ply in positions:
            turn = first_turn * (-1 if (ply - case["start_ply"]) % 2 else 1)
            rows = [r for r in data["rows"] if r["ply"] == ply]
            data["comparisons"].append(dict(ply=ply, results=compare(rows, turn)))
        # One common location nominated before seeing audit results. All strategies' unique
        # choices compete against the recorded move under the same continuation policy.
        rollout_ply = ranked[0] if ranked else case["anchor_ply"]
        _, played, proposals = next(p for p in plans if p[0] == rollout_ply)
        data.setdefault("rollout_ply", rollout_ply)
        data.setdefault("rollouts", [dict(move=m, played=m == played, continuation=[], steps={}) for m in proposals])
        rollout_evaluate(
            oracle,
            data["rollouts"],
            actual[: rollout_ply - case["start_ply"]],
            save,
        )
        turn = first_turn * (-1 if (rollout_ply - case["start_ply"]) % 2 else 1)
        data["rollout_results"] = rollout_summarize(data["rollouts"], turn)
        data["finished"] = not STOP.is_set()
        data["work_complete"] = data["finished"]
        data["limitation"] = (
            "Pooled-candidate search and depth-three rollout estimates, not solved labels; "
            "unsearched alternatives remain possible. Rollout endpoints are after four/eight replies to the candidate; "
            "shallow_steps flags incomplete depth-three continuations. Both horizons and trajectories must be inspected."
        )
        save()
    finally:
        save()
        oracle.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("config", type=Path)
    parser.add_argument("--case", required=True)
    args = parser.parse_args()
    config = json.loads(args.config.read_text())
    validate_inputs(config)
    case = next(c for c in config["cases"] if c["id"] == args.case)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: STOP.set())
    run(config, case)
