"""Paired, fixed-horizon continuations. Never attach the source game's result to a branch."""

from run import STOP


def evaluate(oracle, rows, prefix, save, seconds=6):
    """Advance candidates round-robin; checkpoint every iteration and every selected move.

    Offsets count replies AFTER the nominated move. Both horizon four and horizon
    eight therefore have identical side-to-move parity across candidates. The
    search at an endpoint also chooses the next move when continuing to eight.
    """
    for offset in range(9):
        for row in rows:
            if row.get("terminal") is not None or row.get("blocked"):
                continue
            if len(row["continuation"]) != offset:
                continue  # Already persisted this step on an earlier run.
            records = row["steps"].setdefault(str(offset), dict(records={}))
            if not records.get("done"):
                if STOP.is_set():
                    return

                def receive(event, record=records):
                    if event["event"] == "iteration":
                        record["records"][str(event["depth"])] = event
                        save()

                result = oracle.call(
                    dict(
                        op="search",
                        path=prefix + [row["move"]] + row["continuation"],
                        depth=3,
                        time_ms=max(1, int(seconds * 1000)),
                    ),
                    receive,
                )
                records.update(done=True, result=result)
                if result.get("terminal") is not None:
                    row.update(terminal=result["terminal"], terminal_after=offset)
                save()
            # Also handle an interruption between recording a terminal result and advancing.
            if records["result"].get("terminal") is not None:
                row.update(terminal=records["result"]["terminal"], terminal_after=offset)
                save()
                continue
            completed = max(map(int, records["records"]), default=0)
            if not completed:
                row["blocked"] = "no completed iteration"
            elif offset < 8:
                best = records["records"][str(completed)]["best"]
                row["continuation"].append(best)
            else:
                row["finished"] = True
            save()


def endpoint(row, horizon):
    if row.get("terminal") is not None and row["terminal_after"] <= horizon:
        return dict(terminal=row["terminal"], score_black=row["terminal"] * 1000000, depth=None)
    step = row["steps"].get(str(horizon), {})
    records = step.get("records", {})
    if not step.get("done") or not records:
        return None
    depth = max(map(int, records))
    return dict(terminal=None, score_black=records[str(depth)]["score_black"], depth=depth)


def summarize(rows, turn):
    played = next((r for r in rows if r["played"]), None)
    results = []
    for row in rows:
        scores = []
        for step in row["steps"].values():
            records = step["records"]
            if records:
                scores.append(records[str(max(map(int, records)))]["score_black"])
        summary = dict(
            move=row["move"],
            played=row["played"],
            horizons={},
            trajectory_black=scores,
            shallow_steps=sum(
                1
                for s in row["steps"].values()
                if s.get("done") and "3" not in s["records"] and s["result"].get("terminal") is None
            ),
        )
        for horizon in (4, 8):
            a = endpoint(row, horizon)
            b = endpoint(played, horizon) if played else None
            # No apparent gain from comparing a shallow endpoint against a deeper one.
            comparable = (
                a is not None
                and b is not None
                and (a["terminal"] is not None or b["terminal"] is not None or a["depth"] == b["depth"])
            )
            summary["horizons"][str(horizon)] = dict(
                endpoint=a,
                gain=turn * (a["score_black"] - b["score_black"]) if comparable else None,
                comparable_endpoint=comparable,
            )
        results.append(summary)
    return results
