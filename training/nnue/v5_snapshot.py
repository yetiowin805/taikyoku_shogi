"""Freeze completed games and currently available conditional-teacher labels."""

import argparse
import hashlib
import json
import shutil
import sqlite3
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "deploy"))
from conditional_teachers import select


def digest(data):
    return hashlib.sha256(data).hexdigest()


def snapshot(config):
    out = Path(config["out"]) / "snapshot"
    out.mkdir(parents=True, exist_ok=True)
    if (out / "snapshot.json").exists():
        return
    run = Path(config["run"])
    state_path = out / "state.json"
    if not state_path.exists():
        shutil.copyfile(run / "state.json", state_path)
    state = json.loads(state_path.read_text())
    manifest = dict(version=1, games=[], labels=["labels.jsonl"])
    games = {}
    for slot in state["slots"]:
        if slot["status"] != "done" or not slot.get("game_path"):
            continue
        path = Path(slot["game_path"])
        if not path.is_absolute():
            path = Path(config["repo"]) / path
        data = path.read_bytes()
        game = json.loads(data)
        if game.get("abort_reason") or game.get("result") not in ("BlackWins", "WhiteWins", "Draw"):
            continue
        h = digest(data)
        dest = out / "games" / path.name
        dest.parent.mkdir(exist_ok=True)
        dest.write_bytes(data)
        games[h] = {k: game[k] for k in ("result", "black", "white")}
        games[h]["moves"] = [{k: m[k] for k in ("color", "eval") if k in m} for m in game["moves"]]
        manifest["games"].append(
            dict(path=str(dest.relative_to(out)), sha256=h, run=run.name, pair_seed=slot["start_seed"])
        )
    counts = Counter()
    bindings = json.loads((run / "analysis/dual-label-config.json").read_text())["old"]["models"]
    primary = digest(Path(config["parent"]).read_bytes())
    uri = f"file:{run / 'analysis/training-labels.sqlite'}?mode=ro"
    with sqlite3.connect(uri, uri=True) as db, (out / "labels.jsonl.tmp").open("w") as f:
        db.execute("BEGIN")
        for (payload,) in db.execute("SELECT payload FROM moments WHERE status='completed' ORDER BY id"):
            moment = json.loads(payload)
            game = games.get(moment["game_hash"])
            if game is None:
                continue
            secondary, evidence = select(game, moment["center_ply"])
            wanted = {primary}
            if secondary:
                model = Path(secondary["model"])
                key = str((model if model.is_absolute() else Path(config["repo"]) / model).resolve())
                if key not in bindings:
                    raise ValueError("Missing secondary binding: " + key)
                wanted.add(bindings[key]["sha256"])
            searches = [
                s
                for s in moment.get("previous_teacher_searches", []) + moment.get("searches", [])
                if s.get("model_sha256") in wanted
                and s.get("ply") == moment["center_ply"]
                and s.get("game_hash", moment["game_hash"]) == moment["game_hash"]
                and isinstance(s.get("score"), (int, float))
                and s.get("completed_depth", 0) > 0
            ]
            if not any(s.get("model_sha256") == primary for s in searches):
                counts["positions_without_primary_use_saved"] += 1
                continue
            counts["positions_with_primary"] += 1
            if secondary and len(wanted) > 1 and not any(s.get("model_sha256") != primary for s in searches):
                counts["qualifying_secondary_not_cached"] += 1
            # Keep only label provenance and the last two iterations for stability diagnostics.
            slim = []
            for s in searches:
                keys = (
                    "model_sha256",
                    "score",
                    "completed_depth",
                    "ply",
                    "game_hash",
                    "score_perspective",
                    "agent",
                    "engine_sha256",
                    "analyzer_sha256",
                    "key",
                    "elapsed_ms",
                    "hard_timeout",
                    "best_move",
                )
                slim.append(dict({k: s[k] for k in keys if k in s}, iterations=s.get("iterations", [])[-2:]))
            f.write(
                json.dumps(
                    dict(
                        game_hash=moment["game_hash"],
                        searches=slim,
                        selection=moment.get("selection"),
                        teacher_selection=evidence,
                    )
                )
                + "\n"
            )
    (out / "labels.jsonl.tmp").replace(out / "labels.jsonl")
    manifest.update(
        teacher_policy="512v4-plus-conditional-cached-v1",
        primary_sha256=primary,
        diagnostics=counts,
        state_sha256=digest(state_path.read_bytes()),
    )
    (out / "snapshot.json.tmp").write_text(json.dumps(manifest, indent=2) + "\n")
    (out / "snapshot.json.tmp").replace(out / "snapshot.json")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("config", type=Path)
    a = p.parse_args()
    snapshot(json.loads(a.config.read_text()))
