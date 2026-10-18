"""Print compact pilot progress and write machine-readable, provisional comparisons."""

import argparse
import json
import statistics
from pathlib import Path

from comparison import STRATEGIES

from run import atomic


def report(out):
    status = out / "status.json"
    print(status.read_text() if status.exists() else "No supervisor status yet")
    rows = []
    matched = []
    for path in sorted(out.glob("*/audit/audit.json")):
        audit = json.loads(path.read_text())
        if "positions" in audit:
            arms = {s: json.loads((path.parent.parent / s / "summary.json").read_text()) for s in STRATEGIES}
            resumed = [s for s, summary in arms.items() if summary.get("resumes")]
            for row in audit.get("comparisons", []):
                matched.append(
                    dict(
                        row,
                        case=audit["case"]["id"],
                        cohort=audit["case"].get("cohort", "unspecified"),
                        leader=(audit.get("leader") or {}).get("id"),
                        resumed_arms=resumed,
                        primary_timing_eligible=not resumed,
                        compute={
                            s: dict(
                                total_cpu_seconds=summary["spent"],
                                checkpoint_cpu_seconds=summary["snapshots"][str(row["checkpoint"])]["cpu_seconds"],
                            )
                            for s, summary in arms.items()
                        },
                    )
                )
            continue
        for strategy in ("local", "pv", "contested", "refutation"):
            summary_path = path.parent.parent / strategy / "summary.json"
            if not summary_path.exists():
                continue
            summary = json.loads(summary_path.read_text())
            top = summary["diagnosis"][0]
            checked = next((c for c in audit.get("comparisons", []) if c["ply"] == top["ply"]), {})
            result = next((r for r in checked.get("results", []) if r["move"] == top["best"]), None)
            rollout = next((r for r in audit.get("rollout_results", []) if r["move"] == top["best"]), None)
            rows.append(
                dict(
                    case=audit["case"]["id"],
                    strategy=strategy,
                    spent=summary["spent"],
                    top=top,
                    audit=result,
                    rollout=rollout if audit.get("rollout_ply") == top["ply"] else None,
                )
            )
    aggregates = []
    groups = sorted({(r["cohort"], r["strategy"], r["checkpoint"]) for r in matched})
    for cohort, strategy, checkpoint in groups:
        selected = [
            r for r in matched if (r["cohort"], r["strategy"], r["checkpoint"]) == (cohort, strategy, checkpoint)
        ]
        resumed_rows = sum(not r["primary_timing_eligible"] for r in selected)
        selected = [r for r in selected if r["primary_timing_eligible"]]
        record = dict(
            cohort=cohort,
            strategy=strategy,
            checkpoint=checkpoint,
            rows=len(selected),
            excluded_resumed_rows=resumed_rows,
            missing_recommendations=sum("missing" in r for r in selected),
            changed_moves=sum(not r.get("same_move", True) for r in selected),
            horizons={},
        )
        for h in ("4", "8"):
            per_case = {}
            for r in selected:
                gain = r.get("horizons", {}).get(h, {}).get("gain")
                if gain is not None:
                    per_case.setdefault(r["case"], []).append(gain)
            record["horizons"][h] = dict(
                cases=len(per_case),
                comparable_positions=sum(map(len, per_case.values())),
                equal_case_mean_gain=statistics.mean(statistics.mean(g) for g in per_case.values())
                if per_case
                else None,
            )
        aggregates.append(record)
    atomic(
        out / "comparison.json",
        dict(
            rows=rows,
            matched_rows=matched,
            aggregates=aggregates,
            limitation="Search/rollout corroboration, not true blunder labels or measured playing strength. Inspect missing, shallow, terminal, and parity evidence separately.",
        ),
    )
    for aggregate in aggregates:
        print(json.dumps(aggregate))
    for row in rows:
        check = row["audit"]
        print(
            row["case"],
            row["strategy"],
            "ply",
            row["top"]["ply"],
            "paired-parity gain",
            check["robust_gain"] if check else "not checked",
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("out", type=Path)
    report(parser.parse_args().out)
