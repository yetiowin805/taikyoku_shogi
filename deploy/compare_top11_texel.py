#!/usr/bin/env python3
"""Compare mix top-11 chassis piece tables to their Texel fits.

Prints /Pawn %Δ (scale-free) so T150 vs H120 inits are comparable even when
`texel-fit` renormalizes Pawn to 1. PST / tropism / k are not trained.

  python3 deploy/compare_top11_texel.py \\
    --grid models/top4-mix-grid \\
    --fits models/top11-texel \\
    --out models/top11-texel/compare.md

  python3 deploy/compare_top11_texel.py --list-ids
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple

# Mix-tournament top 11 (not leftover-only history). Keep in sync with FUTURE_NOTES.md.
CHASSIS_IDS: Tuple[str, ...] = (
    "T150_P120_T12",
    "H120_P120_T15",
    "AVG_T150_H120",
    "C2K50A1",
    "BASE_P120H50B75",
    "BASE_H120O80",
    "SEED",
    "H120_B65_T12",
    "AVG_P120_SEED",
    "T150_B65_T12",
    "C2K100A1D50",
)

# Pieces the twin transplant would overwrite. Capturers + range two-movers.
HEADLINE: Tuple[str, ...] = (
    "HookMover",
    "Capricorn",
    "Tengu",
    "Peacock",
    "GreatGeneral",
    "ViceGeneral",
    "BishopGeneral",
    "FreeKing",
    "FierceDragon",
)

# Init |value| at or above this counts as "large" for the group stats.
LARGE_FLOOR = 400.0

SHORT: Dict[str, str] = {
    "T150_P120_T12": "T150P12",
    "H120_P120_T15": "H120P15",
    "AVG_T150_H120": "AVG_TH",
    "C2K50A1": "C2K50",
    "BASE_P120H50B75": "P120H50",
    "BASE_H120O80": "H120O80",
    "SEED": "SEED",
    "H120_B65_T12": "H120B65",
    "AVG_P120_SEED": "AVG_PS",
    "T150_B65_T12": "T150B65",
    "C2K100A1D50": "C2D50",
}


def load_cp(path: Path) -> dict:
    return json.loads(path.read_text())


def pieces(cp: dict) -> Dict[str, float]:
    return {k: float(v) for k, v in cp["weights"]["piece"].items()}


def pawn(piece: Dict[str, float]) -> float:
    return max(abs(piece.get("Pawn", 1.0)), 1e-6)


def per_pawn(piece: Dict[str, float]) -> Dict[str, float]:
    p = pawn(piece)
    return {k: v / p for k, v in piece.items()}


def pct_delta(before: float, after: float) -> float:
    base = max(abs(before), 1e-6)
    return 100.0 * (after - before) / base


def mean(xs: Sequence[float]) -> float:
    return sum(xs) / len(xs) if xs else 0.0


def stdev(xs: Sequence[float]) -> float:
    if len(xs) < 2:
        return 0.0
    m = mean(xs)
    return math.sqrt(sum((x - m) ** 2 for x in xs) / (len(xs) - 1))


def is_large(name: str, init_val: float) -> bool:
    return name in HEADLINE or abs(init_val) >= LARGE_FLOOR


def fit_path(fits: Path, chassis_id: str) -> Path:
    return fits / f"{chassis_id}-texel.json"


def chassis_path(grid: Path, chassis_id: str) -> Path:
    return grid / f"{chassis_id}.json"


def parse_log(path: Path) -> Dict[str, Any]:
    out: Dict[str, Any] = {}
    if not path.is_file():
        return out
    text = path.read_text()
    for line in text.splitlines():
        if "CE " in line and "used=" in line:
            out["wrote"] = line.strip()
    return out


def agent_stats(init: Dict[str, float], fit: Dict[str, float]) -> Dict[str, Any]:
    names = sorted(set(init) | set(fit))
    r0 = per_pawn(init)
    r1 = per_pawn(fit)
    all_pct = []
    large_pct = []
    small_pct = []
    rows = []
    for name in names:
        a = init.get(name, 0.0)
        b = fit.get(name, 0.0)
        d = pct_delta(r0.get(name, 0.0), r1.get(name, 0.0))
        all_pct.append(abs(d))
        bucket = "large" if is_large(name, a) else "small"
        (large_pct if bucket == "large" else small_pct).append(abs(d))
        rows.append(
            {
                "piece": name,
                "init": a,
                "fit": b,
                "init_pp": r0.get(name, 0.0),
                "fit_pp": r1.get(name, 0.0),
                "pct_pp": d,
                "group": bucket,
            }
        )
    rows.sort(key=lambda r: -abs(r["pct_pp"]))
    return {
        "max_abs_pct_pp": max(all_pct) if all_pct else 0.0,
        "mean_abs_pct_pp": mean(all_pct),
        "large_mean_abs_pct_pp": mean(large_pct),
        "small_mean_abs_pct_pp": mean(small_pct),
        "n_large": len(large_pct),
        "n_small": len(small_pct),
        "rows": rows,
    }


def fmt_pct(x: float) -> str:
    return f"{x:+.1f}"


def fmt_val(x: float) -> str:
    if abs(x) >= 100:
        return f"{x:.0f}"
    return f"{x:.2f}"


def md_table(headers: Sequence[str], rows: Iterable[Sequence[str]]) -> str:
    hs = list(headers)
    body = [list(r) for r in rows]
    lines = [
        "| " + " | ".join(hs) + " |",
        "| " + " | ".join("---" if i == 0 else "---:" for i in range(len(hs))) + " |",
    ]
    for r in body:
        lines.append("| " + " | ".join(r) + " |")
    return "\n".join(lines)


def render(
    grid: Path,
    fits: Path,
    ids: Sequence[str],
) -> Tuple[str, dict]:
    agents = []
    missing: List[str] = []
    for i in ids:
        ip = chassis_path(grid, i)
        fp = fit_path(fits, i)
        if not ip.is_file() or not fp.is_file():
            missing.append(i)
            continue
        init_cp = load_cp(ip)
        fit_cp = load_cp(fp)
        init_p = pieces(init_cp)
        fit_p = pieces(fit_cp)
        st = agent_stats(init_p, fit_p)
        k0 = float(init_cp["weights"].get("two_mover_mob_k") or 0.0)
        k1 = float(fit_cp["weights"].get("two_mover_mob_k") or 0.0)
        log = parse_log(fits / f"{i}-texel.fit.log")
        agents.append(
            {
                "id": i,
                "short": SHORT.get(i, i),
                "stats": st,
                "k_init": k0,
                "k_fit": k1,
                "log": log,
            }
        )

    if missing:
        raise SystemExit(
            "missing chassis or fit for: " + ", ".join(missing) + f"\n  grid={grid}\n  fits={fits}"
        )

    lines: List[str] = []
    lines.append("# Top-11 Texel piece-value deltas")
    lines.append("")
    lines.append(
        f"Init from `{grid}/<id>.json`. Fit from `{fits}/<id>-texel.json`. "
        "Deltas are **/Pawn** so pawn-renorm does not look like a table rewrite. "
        f"Large = headline capturers/two-movers, or init `|value| ≥ {LARGE_FLOOR:.0f}`."
    )
    lines.append("")
    lines.append("## Per chassis")
    lines.append("")
    sum_rows = []
    for a in agents:
        st = a["stats"]
        sum_rows.append(
            [
                a["id"],
                f"{st['max_abs_pct_pp']:.1f}",
                f"{st['mean_abs_pct_pp']:.1f}",
                f"{st['large_mean_abs_pct_pp']:.1f}",
                f"{st['small_mean_abs_pct_pp']:.1f}",
                f"{a['k_init']:.1f}",
                f"{a['k_fit']:.1f}",
            ]
        )
    lines.append(
        md_table(
            [
                "Chassis",
                "max |%Δ| /P",
                "mean |%Δ| /P",
                "large mean",
                "small mean",
                "k init",
                "k fit",
            ],
            sum_rows,
        )
    )
    lines.append("")
    lines.append(
        "`k` is not in the Texel feature vector — init and fit should match. "
        "If small-piece mean |%Δ| is close to large, a large-only transplant is doing real work "
        "by *not* dragging the mid table together."
    )
    lines.append("")

    lines.append("## Headline /Pawn %Δ")
    lines.append("")
    hdr = ["Piece"] + [a["short"] for a in agents]
    body = []
    for piece in HEADLINE:
        row = [piece]
        for a in agents:
            hit = next((r for r in a["stats"]["rows"] if r["piece"] == piece), None)
            row.append(fmt_pct(hit["pct_pp"]) if hit else "—")
        body.append(row)
    lines.append(md_table(hdr, body))
    lines.append("")
    lines.append("Short ids: " + ", ".join(f"{a['short']}={a['id']}" for a in agents))
    lines.append("")

    lines.append("## After-fit /Pawn (did they collapse?)")
    lines.append("")
    lines.append(
        "If Hook/GG land on almost the same /Pawn from every init, one shared piece "
        "table is enough. Wide spread means the chassis prior still matters."
    )
    lines.append("")
    body = []
    collapse = []
    for piece in HEADLINE:
        vals = []
        row = [piece]
        for a in agents:
            hit = next((r for r in a["stats"]["rows"] if r["piece"] == piece), None)
            if hit:
                vals.append(hit["fit_pp"])
                row.append(fmt_val(hit["fit_pp"]))
            else:
                row.append("—")
        body.append(row)
        if vals:
            collapse.append((piece, mean(vals), stdev(vals), min(vals), max(vals)))
    lines.append(md_table(hdr, body))
    lines.append("")
    lines.append(
        md_table(
            ["Piece", "mean /P", "stdev", "min", "max"],
            [
                [p, fmt_val(m), fmt_val(s), fmt_val(lo), fmt_val(hi)]
                for p, m, s, lo, hi in collapse
            ],
        )
    )
    lines.append("")

    lines.append("## Biggest movers per chassis (top 8 by |%Δ| /Pawn)")
    lines.append("")
    for a in agents:
        lines.append(f"### {a['id']}")
        lines.append("")
        top = a["stats"]["rows"][:8]
        lines.append(
            md_table(
                ["Piece", "group", "init /P", "fit /P", "%Δ /P"],
                [
                    [
                        r["piece"],
                        r["group"],
                        fmt_val(r["init_pp"]),
                        fmt_val(r["fit_pp"]),
                        fmt_pct(r["pct_pp"]),
                    ]
                    for r in top
                ],
            )
        )
        lines.append("")
        wrote = a["log"].get("wrote")
        if wrote:
            lines.append(f"Log: `{wrote}`")
            lines.append("")

    payload = {
        "grid": str(grid),
        "fits": str(fits),
        "chassis": [a["id"] for a in agents],
        "headline": list(HEADLINE),
        "large_floor": LARGE_FLOOR,
        "agents": [
            {
                "id": a["id"],
                "k_init": a["k_init"],
                "k_fit": a["k_fit"],
                "max_abs_pct_pp": a["stats"]["max_abs_pct_pp"],
                "mean_abs_pct_pp": a["stats"]["mean_abs_pct_pp"],
                "large_mean_abs_pct_pp": a["stats"]["large_mean_abs_pct_pp"],
                "small_mean_abs_pct_pp": a["stats"]["small_mean_abs_pct_pp"],
                "headline": {
                    r["piece"]: {
                        "init_pp": r["init_pp"],
                        "fit_pp": r["fit_pp"],
                        "pct_pp": r["pct_pp"],
                    }
                    for r in a["stats"]["rows"]
                    if r["piece"] in HEADLINE
                },
            }
            for a in agents
        ],
    }
    return "\n".join(lines) + "\n", payload


def main(argv: Optional[Sequence[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--grid", default="models/top4-mix-grid", type=Path)
    ap.add_argument("--fits", default="models/top11-texel", type=Path)
    ap.add_argument("--out", type=Path, default=None)
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--list-ids", action="store_true")
    args = ap.parse_args(argv)

    if args.list_ids:
        print("\n".join(CHASSIS_IDS))
        return 0

    md, payload = render(args.grid, args.fits, CHASSIS_IDS)
    out = args.out or (args.fits / "compare.md")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(md)
    json_out = args.json_out or (args.fits / "compare.json")
    json_out.write_text(json.dumps(payload, indent=2) + "\n")
    sys.stdout.write(md)
    print(f"Wrote {out}", file=sys.stderr)
    print(f"Wrote {json_out}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
