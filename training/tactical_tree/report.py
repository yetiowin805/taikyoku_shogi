"""Print compact pilot progress and write machine-readable, provisional comparisons."""

import argparse
import json
from pathlib import Path

from run import atomic


def report(out):
    status = out / "status.json"
    print(status.read_text() if status.exists() else "No supervisor status yet")
    rows = []
    for path in sorted(out.glob("*/audit/audit.json")):
        audit = json.loads(path.read_text())
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
    atomic(
        out / "comparison.json", dict(rows=rows, limitation="Search/rollout corroboration, not true blunder labels.")
    )
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
