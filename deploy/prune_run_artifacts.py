#!/usr/bin/env python3
"""Reclaim disk from redundant tournament-run artifacts.

Dry run by default; pass --apply to delete/rewrite. Safe while a run is live.

  * analysis/game-workers/slot-*.result.iterations.json: deleted only when the
    saved game already carries the same per-move iteration timings.
  * analysis/*.progress.json: deleted only when its search is committed to the
    run's sqlite `searches` table.
  * analysis/moments/*.json: write-only copies of SQLite moment payloads;
    deleted only when the moment row exists in the run's catalogue.

Game files are never rewritten: their byte SHA-256 identifies them to label
catalogues and NNUE datasets. Logs are reported, never touched.

  ./deploy/prune_run_artifacts.py data/raw/tourney/RUN [RUN ...] [--apply]
"""

import argparse
import json
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def absolute(path):
    p = Path(path)
    return p if p.is_absolute() else ROOT / p


def mib(n):
    return f"{n / 2**20:.1f} MiB"


def game_paths(run):
    state = json.loads((run / "state.json").read_text())
    for slot in state.get("slots", []):
        if slot.get("game_path"):
            path = absolute(slot["game_path"])
            if path.is_file():
                yield path


def timings(moves):
    return [m.get("iteration_timings") or [] for m in moves]


def prune_sidecars(run, games, apply):
    freed = kept = 0
    for sidecar in sorted((run / "analysis/game-workers").glob("slot-*.result.iterations.json")):
        try:
            data = json.loads(sidecar.read_text())
            game = games.get(data.get("game_id"))
            redundant = game is not None and timings(json.loads(game.read_text())["moves"]) == timings(data["moves"])
        except (OSError, ValueError, KeyError, TypeError):
            redundant = False
        if not redundant:
            kept += 1
            continue
        freed += sidecar.stat().st_size
        if apply:
            sidecar.unlink(missing_ok=True)
    return freed, kept


def catalogue_keys(run, query):
    keys = set()
    for name in ("catalogue.sqlite", "training-labels.sqlite"):
        path = run / "analysis" / name
        if not path.is_file():
            continue
        with sqlite3.connect(f"file:{path}?mode=ro", uri=True) as db:
            try:
                keys.update(k for (k,) in db.execute(query))
            except sqlite3.OperationalError:
                pass
    return keys


def prune_progress(run, apply):
    keys = catalogue_keys(run, "SELECT key FROM searches")
    freed = kept = 0
    for progress in sorted((run / "analysis").glob("*.progress.json")):
        if progress.name[: -len(".progress.json")] not in keys:
            kept += 1
            continue
        freed += progress.stat().st_size
        if apply:
            progress.unlink(missing_ok=True)
    return freed, kept


def prune_moments(run, apply):
    ids = catalogue_keys(run, "SELECT id FROM moments")
    freed = kept = 0
    for moment in sorted((run / "analysis/moments").glob("*.json")):
        if moment.stem not in ids:
            kept += 1
            continue
        freed += moment.stat().st_size
        if apply:
            moment.unlink(missing_ok=True)
    return freed, kept


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("runs", nargs="+", type=Path)
    parser.add_argument("--apply", action="store_true", help="actually delete/rewrite (default: dry run)")
    args = parser.parse_args()
    verb = "freed" if args.apply else "would free"
    total = 0
    for run in args.runs:
        run = run.resolve()
        if not (run / "state.json").is_file():
            sys.exit(f"prune_run_artifacts: {run} has no state.json; not a tournament run")
        paths = list(game_paths(run))
        games = {}
        for path in paths:
            try:
                games[json.loads(path.read_text())["game_id"]] = path
            except (OSError, ValueError, KeyError):
                pass
        print(f"{run}:")
        freed, kept = prune_sidecars(run, games, args.apply)
        total += freed
        print(f"  iteration sidecars: {verb} {mib(freed)} (kept {kept} not covered by a saved game)")
        freed, kept = prune_progress(run, args.apply)
        total += freed
        print(f"  search progress:    {verb} {mib(freed)} (kept {kept} not yet committed)")
        freed, kept = prune_moments(run, args.apply)
        total += freed
        print(f"  moment copies:      {verb} {mib(freed)} (kept {kept} not in the catalogue)")
        for log in sorted(run.glob("**/*.log")):
            if log.stat().st_size >= 2**20:
                print(f"  log (untouched):    {mib(log.stat().st_size)} {log.relative_to(run)}")
    print(f"total {verb} {mib(total)}" + ("" if args.apply else "; rerun with --apply"))


if __name__ == "__main__":
    main()
