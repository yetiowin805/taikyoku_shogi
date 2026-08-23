#!/usr/bin/env python3
"""Print standings and knockout trees for a tourney run.

  ./deploy/tourney_status.py                 # latest run under data/raw/tourney
  ./deploy/tourney_status.py top11-c2        # latest run whose id contains that
  ./deploy/tourney_status.py --run-id ID
  ./deploy/tourney_status.py --standings     # table only
  ./deploy/tourney_status.py --tree          # trees only
  ./deploy/tourney_status.py --tree 1        # one tree (1-based or tree id)
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Tuple

DEFAULT_TOURNEY_ROOT = Path("data/raw/tourney")


def stage_label(stage: Any) -> str:
    if stage == "PlayIn" or stage == {"PlayIn": None}:
        return "PlayIn"
    if isinstance(stage, dict):
        rnd = stage.get("Round")
        if isinstance(rnd, dict):
            size = int(rnd.get("size", 0))
            return "Final" if size == 2 else f"R{size}"
    return str(stage)


def stage_sort_key(label: str) -> Tuple[int, int]:
    if label == "PlayIn":
        return (0, 0)
    if label == "Final":
        return (2, 0)
    if label.startswith("R"):
        try:
            return (1, -int(label[1:]))
        except ValueError:
            return (3, 0)
    return (3, 0)


def slot_by_id(state: Dict[str, Any]) -> Dict[int, Dict[str, Any]]:
    return {int(s["id"]): s for s in state.get("slots", [])}


def match_score(
    match: Dict[str, Any], slots: Dict[int, Dict[str, Any]]
) -> Tuple[float, float, Counter]:
    """A-score, B-score, status counts. Only Done slots add to the score."""
    status: Counter = Counter()
    sa = 0.0
    sb = 0.0
    for sid in match.get("slot_ids", []):
        slot = slots.get(int(sid))
        if slot is None:
            status["missing"] += 1
            continue
        st = str(slot.get("status", "?")).lower()
        status[st] += 1
        if st == "done" and slot.get("score_a") is not None:
            a = float(slot["score_a"])
            sa += a
            sb += 1.0 - a
    return sa, sb, status


def format_status(status: Counter, winner: Optional[str]) -> str:
    if winner:
        return f"→ {winner}"
    parts = []
    done = status.get("done", 0)
    running = status.get("running", 0)
    pending = status.get("pending", 0)
    aborted = status.get("aborted", 0)
    total = sum(status.values())
    if total:
        parts.append(f"{done}/{total} done")
    if running:
        parts.append(f"{running} live")
    if pending:
        parts.append(f"{pending} queued")
    if aborted:
        parts.append(f"{aborted} aborted")
    return " ".join(parts) if parts else "unplayed"


def find_runs(root: Path) -> List[Path]:
    if not root.is_dir():
        return []
    runs = []
    for p in root.iterdir():
        if p.is_dir() and (p / "state.json").is_file():
            runs.append(p)
    runs.sort(key=lambda p: (p / "state.json").stat().st_mtime)
    return runs


def pick_run(root: Path, run_id: Optional[str], needle: Optional[str]) -> Path:
    runs = find_runs(root)
    if not runs:
        raise SystemExit(f"no tourney runs with state.json under {root}")
    if run_id:
        for p in reversed(runs):
            if p.name == run_id:
                return p
        raise SystemExit(f"no run {run_id!r} under {root}")
    if needle:
        hits = [p for p in runs if needle in p.name]
        if not hits:
            raise SystemExit(f"no run matching {needle!r} under {root}")
        return hits[-1]
    return runs[-1]


def load_state(run_dir: Path) -> Dict[str, Any]:
    return json.loads((run_dir / "state.json").read_text())


def print_header(run_dir: Path, state: Dict[str, Any]) -> None:
    slots = state.get("slots", [])
    c = Counter(str(s.get("status", "?")).lower() for s in slots)
    trees = state.get("knockouts", [])
    titles = state.get("knockout_titles") or {}
    title_bits = ", ".join(f"{k}×{v}" for k, v in sorted(titles.items(), key=lambda kv: -kv[1]))
    print(f"run={run_dir.name}  format={state.get('format')}  path={run_dir}")
    print(
        f"slots={len(slots)}  "
        + " ".join(f"{k}={v}" for k, v in sorted(c.items()))
    )
    if trees:
        print(
            f"trees={len(trees)}  complete={sum(1 for t in trees if t.get('complete'))}"
            + (f"  titles={title_bits}" if title_bits else "")
        )
    print()


def print_standings(run_dir: Path) -> None:
    path = run_dir / "standings.md"
    if path.is_file():
        text = path.read_text().rstrip()
        print(text)
        print()
        return
    print("(no standings.md yet)\n")


def print_tree(tree: Dict[str, Any], slots: Dict[int, Dict[str, Any]]) -> None:
    tid = tree.get("id", "?")
    seeds: List[str] = list(tree.get("seeds") or [])
    complete = bool(tree.get("complete"))
    champ = None
    for m in tree.get("matches", []):
        if stage_label(m.get("stage")) == "Final" and m.get("winner"):
            champ = m["winner"]
    flag = "complete" if complete else "in progress"
    extra = f"  champion={champ}" if champ else ""
    print(f"## Tree {tid}  {flag}  bracket={tree.get('bracket_size')}{extra}")
    if seeds:
        seed_line = "  ".join(f"{i}.{s}" for i, s in enumerate(seeds, 1))
        print(f"seeds: {seed_line}")
    matches = list(tree.get("matches") or [])
    if not matches:
        print("(no matches yet)\n")
        return
    grouped: Dict[str, List[Dict[str, Any]]] = {}
    for m in matches:
        grouped.setdefault(stage_label(m.get("stage")), []).append(m)
    for label in sorted(grouped, key=stage_sort_key):
        print(f"\n{label}")
        for m in sorted(grouped[label], key=lambda x: int(x.get("bracket_slot", 0))):
            sa, sb, status = match_score(m, slots)
            winner = m.get("winner")
            phase = m.get("phase", "")
            phase_bit = f"  {phase}" if phase and not winner else ""
            print(
                f"  [{m.get('bracket_slot', '?')}] "
                f"{m.get('model_a')} vs {m.get('model_b')}  "
                f"{sa:.1f}–{sb:.1f}  "
                f"{format_status(status, winner)}{phase_bit}"
            )
    print()


def print_trees(
    state: Dict[str, Any], which: Optional[str]
) -> None:
    trees: List[Dict[str, Any]] = list(state.get("knockouts") or [])
    if not trees:
        print("(no knockout trees — Swiss / RR run)\n")
        return
    slots = slot_by_id(state)
    if which is not None:
        picked = []
        for t in trees:
            if str(t.get("id")) == which or str(trees.index(t) + 1) == which:
                picked.append(t)
        if not picked:
            ids = ", ".join(str(t.get("id")) for t in trees)
            raise SystemExit(f"no tree {which!r} (have {ids})")
        trees = picked
    for t in trees:
        print_tree(t, slots)


def self_test() -> None:
    assert stage_label("PlayIn") == "PlayIn"
    assert stage_label({"Round": {"size": 32}}) == "R32"
    assert stage_label({"Round": {"size": 2}}) == "Final"
    assert stage_sort_key("PlayIn") < stage_sort_key("R32") < stage_sort_key("Final")
    slots = {
        1: {"id": 1, "status": "Done", "score_a": 1.0},
        2: {"id": 2, "status": "Done", "score_a": 0.0},
        3: {"id": 3, "status": "Running"},
    }
    sa, sb, st = match_score({"slot_ids": [1, 2, 3]}, slots)
    assert abs(sa - 1.0) < 1e-9 and abs(sb - 1.0) < 1e-9
    assert st["done"] == 2 and st["running"] == 1
    print("self-test ok")


def main(argv: Optional[Iterable[str]] = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("needle", nargs="?", help="substring of run id (picks latest match)")
    ap.add_argument("--root", type=Path, default=DEFAULT_TOURNEY_ROOT)
    ap.add_argument("--run-id", help="exact run directory name")
    ap.add_argument("--standings", action="store_true", help="print standings.md only")
    ap.add_argument("--tree", nargs="?", const="all", metavar="N", help="print trees (optional id)")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(list(argv) if argv is not None else None)

    if args.self_test:
        self_test()
        return 0

    run_dir = pick_run(args.root, args.run_id, args.needle)
    state = load_state(run_dir)
    # Default both. Either flag alone hides the other; both flags keep both.
    want_standings = args.standings or args.tree is None
    want_tree = args.tree is not None or not args.standings

    print_header(run_dir, state)
    if want_standings:
        print_standings(run_dir)
    if want_tree:
        which = None if args.tree in (None, "all") else args.tree
        print_trees(state, which)
    return 0


if __name__ == "__main__":
    sys.exit(main())
