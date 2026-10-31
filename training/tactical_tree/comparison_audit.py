"""Common candidate searches and continuations at locations fixed before arm results."""

import argparse
import json
import signal
import time
from pathlib import Path

from comparison import STRATEGIES
from rollout import evaluate
from tree import key

from audit import compare
from run import STOP, Oracle, atomic, validate_inputs


def endpoint_pair(candidate, baseline, horizon, turn):
    terminals = [
        r.get("terminal") if r.get("terminal_after", horizon + 1) <= horizon else None for r in (candidate, baseline)
    ]
    if any(t is not None for t in terminals):
        # Actual outcomes and heuristic mate scores are different types of evidence.
        return dict(
            terminal_outcomes=terminals,
            depth=None,
            gain=None,
            wdl_gain=turn * (terminals[0] - terminals[1]) if all(t is not None for t in terminals) else None,
        )
    steps = [r["steps"].get(str(horizon), {}) for r in (candidate, baseline)]
    common = set(steps[0].get("records", {})) & set(steps[1].get("records", {}))
    if not all(s.get("done") for s in steps) or not common:
        return dict(depth=None, gain=None, reason="missing comparable endpoint")
    depth = max(map(int, common))
    scores = [s["records"][str(depth)]["score_black"] for s in steps]
    return dict(depth=depth, scores_black=scores, gain=turn * (scores[0] - scores[1]), shallow=depth < 3)


def proposal_at(summary, point, ply):
    snapshot = summary["snapshots"][str(point)]
    return next((r.get("best") for r in snapshot["recommendations"] if r["ply"] == ply), None)


def run(config, case):
    out = Path(config["out"]) / case["id"] / "audit"
    out.mkdir(parents=True, exist_ok=True)
    path = out / "audit.json"
    data = (
        json.loads(path.read_text())
        if path.exists()
        else dict(
            case=case,
            identities=config["identities"],
            leader=config.get("leader"),
            positions={},
            spent=0,
        )
    )
    if data["case"] != case or data["identities"] != config["identities"]:
        raise ValueError("audit identity mismatch")
    if data.get("work_complete"):
        return
    summaries = {s: json.loads((out.parent / s / "summary.json").read_text()) for s in STRATEGIES}
    if not all(s["finished"] and s["leader"] == config.get("leader") for s in summaries.values()):
        raise ValueError("audit requires all arms completed with the same model")
    oracle = Oracle(config, case, out)
    actual = oracle.ready["actual"]
    positions = case.get("audit_plies", [case["anchor_ply"] + i for i in (-2, 0, 2)])
    if any(not case["start_ply"] <= p < case["end_ply"] for p in positions):
        raise ValueError("preselected audit position outside episode")
    prior, start = data["spent"], time.monotonic()

    def save():
        data["spent"] = prior + time.monotonic() - start
        atomic(path, data)

    def query(row, branch, seconds):
        if row.get("done"):
            return

        def receive(event):
            if event["event"] == "iteration":
                row["records"][str(event["depth"])] = event
                save()

        row["result"] = oracle.call(dict(op="search", path=branch, depth=6, time_ms=int(seconds * 1000)), receive)
        row["done"] = True
        save()

    try:
        for ply in positions:
            if STOP.is_set():
                return
            prefix = actual[: ply - case["start_ply"]]
            played = actual[ply - case["start_ply"]]
            entry = data["positions"].setdefault(str(ply), dict(root=dict(records={}), children={}))
            query(entry["root"], prefix, 30)
            proposals = {key(played): played}
            records = entry["root"]["records"]
            if records:
                move = records[str(max(map(int, records)))]["best"]
                if move is not None:
                    proposals[key(move)] = move
            for summary in summaries.values():
                for point in config["checkpoints"]:
                    move = proposal_at(summary, point, ply)
                    if move is not None:
                        proposals[key(move)] = move
            for move_id, move in proposals.items():
                row = entry["children"].setdefault(
                    move_id,
                    dict(
                        move=move, records={}, rollout=dict(move=move, played=move == played, continuation=[], steps={})
                    ),
                )
                query(row, prefix + [move], 25)
            evaluate(oracle, [r["rollout"] for r in entry["children"].values()], prefix, save)
        data["comparisons"] = []
        for ply in positions:
            turn = (1 if case["start_color"] == "Black" else -1) * (-1 if (ply - case["start_ply"]) % 2 else 1)
            children = data["positions"][str(ply)]["children"]
            for point in config["checkpoints"]:
                baseline = proposal_at(summaries["uniform"], point, ply)
                for strategy in STRATEGIES[1:]:
                    move = proposal_at(summaries[strategy], point, ply)
                    result = dict(ply=ply, checkpoint=point, strategy=strategy, move=move, baseline=baseline)
                    if move is None or baseline is None:
                        result["missing"] = "no completed recommendation at checkpoint"
                    else:
                        a, b = children[key(move)], children[key(baseline)]
                        result["same_move"] = move == baseline
                        result["search"] = compare([dict(a, played=False), dict(b, played=True)], turn)[0]
                        result["terminals"] = [r["result"].get("terminal") for r in (a, b)]
                        result["horizons"] = {
                            str(h): endpoint_pair(a["rollout"], b["rollout"], h, turn) for h in (4, 8)
                        }
                    data["comparisons"].append(result)
        data["work_complete"] = not STOP.is_set()
        data["limitation"] = (
            "Same-model follow-up searches and continuations are corroboration, not independent truth or Elo. "
            "All arms share the episode's frozen leader. Missing evidence is not a zero gain. "
            "Inspect both parities, both horizons, shallow continuations, and the complete saved trajectories."
        )
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
