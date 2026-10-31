#!/usr/bin/env python3
"""Prepare bounded, disjoint PGO/replay corpora from a host's rolling run."""

import argparse
import copy
import json
import shutil
from pathlib import Path

import tourney_analysis as analysis


def load_json(data):
    def invalid_constant(value):
        raise ValueError(f"nonfinite JSON constant: {value}")

    return json.loads(data, parse_constant=invalid_constant)


def span_indices(length, count):
    if count < 1 or length < count:
        raise ValueError(f"need {count} distinct inputs, found {length}")
    if count == 1:
        return [length // 2]
    return [i * (length - 1) // (count - 1) for i in range(count)]


def model_identity(checkpoint):
    if not isinstance(checkpoint, dict) or not isinstance(checkpoint.get("weights", {}), dict):
        raise ValueError("model checkpoint and weights must be JSON objects")
    identity = copy.deepcopy(checkpoint)
    descriptor = identity.get("weights", {}).get("nnue")
    if descriptor:
        if not isinstance(descriptor, dict):
            raise ValueError("NNUE descriptor must be a JSON object")
        descriptor["file"] = "<pinned NNUE artifact>"
    return identity


def binding_paths(binding):
    result = copy.deepcopy(binding)
    result["snapshot"] = str(analysis.absolute(result["snapshot"]).resolve())
    for artifact in result.get("artifacts", []):
        for key in ("original", "snapshot"):
            artifact[key] = str(analysis.absolute(artifact[key]).resolve())
    return result


def select_models(state, config, requested, maximum):
    bindings = {}
    for path, value in sorted(config["models"].items()):
        bindings.setdefault(str(analysis.absolute(path).resolve()), []).append(binding_paths(value))
    entrants = state["entrants"]
    retired = set(state.get("retired", []))
    if len({entry["id"] for entry in entrants}) != len(entrants):
        raise ValueError("duplicate entrant IDs in run state")
    eligible = {entry["id"]: entry for entry in entrants if entry["id"] not in retired and not entry.get("engine")}
    if requested is not None:
        if len(set(requested)) != len(requested):
            raise ValueError("duplicate --models entrant IDs")
        missing = set(requested) - eligible.keys()
        if missing:
            raise ValueError(f"requested models are missing, retired or use an engine override: {sorted(missing)}")
        ordered = [eligible[name] for name in requested]
    else:
        ordered = [eligible[name] for name in sorted(eligible)]
    unique = {}
    for entrant in ordered:
        original = analysis.absolute(entrant["model"]).resolve()
        matching = bindings.get(str(original))
        if not matching:
            raise ValueError(f"no frozen model binding for active entrant {entrant['id']}: {original}")
        original_data = original.read_bytes()
        sha = analysis.digest(original_data)
        if any(sha != binding["sha256"] for binding in matching):
            raise ValueError(f"original model changed: {original}")
        checkpoint = load_json(original_data)
        # Every symlink/relative alias must retain its original pin. Validate
        # small JSON content for all matches before choosing a runtime snapshot.
        for binding in matching:
            raw = Path(binding["snapshot"]).read_bytes()
            if analysis.digest(raw) != binding.get("snapshot_sha256", sha):
                raise ValueError(f"snapshot model changed: {binding['snapshot']}")
            if model_identity(checkpoint) != model_identity(load_json(raw)):
                raise ValueError(f"frozen model policy differs from original: {original}")
        binding = matching[0]
        frozen = Path(binding["snapshot"])
        frozen_data = frozen.read_bytes()
        snapshot_sha = binding.get("snapshot_sha256", sha)
        snapshot = load_json(frozen_data)
        defaults = checkpoint.get("search_defaults", {})
        if not isinstance(defaults, dict):
            raise ValueError(f"invalid search defaults: {original}")
        # Hash the entire checkpoint, not just its NNUE blob: two policies using
        # identical network weights remain distinct profiling models.
        if sha in unique:
            unique[sha]["aliases"].append(entrant["id"])
            unique[sha]["_bindings"].extend((str(original), match) for match in matching)
            continue
        descriptor = snapshot.get("weights", {}).get("nnue")
        width = descriptor.get("width", 0) if descriptor else 0
        if descriptor and (type(width) is not int or not 32 <= width <= 4096 or width % 32):
            raise ValueError(f"invalid NNUE width: {original}")
        group = (
            width,
            json.dumps(defaults.get("experiment", "baseline"), sort_keys=True),
            json.dumps(defaults.get("trial", "none"), sort_keys=True),
        )
        item = dict(
            name=entrant["id"],
            aliases=[entrant["id"]],
            path=str(frozen),
            sha256=snapshot_sha,
            source_model=str(original),
            source_sha256=sha,
            search_defaults=defaults,
            selection_group=list(group),
            _bindings=[(str(original), match) for match in matching],
        )
        if descriptor:
            blob = (frozen.parent / descriptor["file"]).resolve()
            item.update(nnue=descriptor, blob_path=str(blob), blob_sha256=descriptor["sha256"])
        unique[sha] = item
    candidates = list(unique.values())
    if not candidates:
        raise ValueError("no eligible active model checkpoints")
    if requested is not None:
        if len(candidates) > maximum:
            raise ValueError(f"requested {len(candidates)} unique checkpoints exceeds --max-models {maximum}")
        selected = candidates
    else:
        groups = {}
        for item in sorted(candidates, key=lambda item: (item["selection_group"], item["name"])):
            groups.setdefault(tuple(item["selection_group"]), []).append(item)
        representatives = [items[0] for items in groups.values()]
        if len(representatives) > maximum:
            selected = [representatives[i] for i in span_indices(len(representatives), maximum)]
        else:
            selected = representatives
            remaining = [item for item in candidates if item not in selected]
            count = min(maximum - len(selected), len(remaining))
            if count:
                selected += [remaining[i] for i in span_indices(len(remaining), count)]
    for item in selected:
        for original, binding in item.pop("_bindings"):
            analysis.validate_model_binding(original, binding)
        if "blob_path" in item and analysis.file_digest(item["blob_path"]) != item["blob_sha256"]:
            raise ValueError(f"frozen NNUE descriptor artifact changed: {item['blob_path']}")
    return selected


def validate_record(record, path):
    if not isinstance(record, dict) or record.get("format_version") != 2:
        raise ValueError(f"expected a version-2 rolling game record: {path}")
    if record.get("abort_reason") or record.get("result") is None:
        return False
    if record["result"] not in ("BlackWins", "WhiteWins", "Draw"):
        raise ValueError(f"invalid game result: {path}")
    if not isinstance(record.get("game_id"), str) or not record["game_id"]:
        raise ValueError(f"missing game identity: {path}")
    if type(record.get("seed")) is not int or not 0 <= record["seed"] < 2**64:
        raise ValueError(f"invalid game seed: {path}")
    start = record.get("start")
    if not isinstance(start, dict) or start.get("kind") not in ("opening", "position"):
        raise ValueError(f"invalid game start: {path}")
    if start["kind"] == "position" and not isinstance(start.get("position"), dict):
        raise ValueError(f"missing initial board position: {path}")
    for side in ("black", "white"):
        if not isinstance(record.get(side), dict) or not isinstance(record[side].get("name"), str):
            raise ValueError(f"invalid {side} agent: {path}")
    moves = record.get("moves")
    if not isinstance(moves, list):
        raise ValueError(f"invalid move list: {path}")
    for number, move in enumerate(moves, 1):
        # Rolling records count a compound turn's component steps, while PGO
        # roots count entries in this array. Replay consumes the array prefix;
        # turn legality belongs to the engine's eager replay, not this shape check.
        if (
            not isinstance(move, dict)
            or type(move.get("move_number")) is not int
            or not 1 <= move["move_number"] < 2**64
        ):
            raise ValueError(f"invalid record move number at ply {number}: {path}")
        color = move.get("color")
        if color not in ("Black", "White"):
            raise ValueError(f"invalid move color at ply {number}: {path}")
        if type(move.get("promoted")) is not bool:
            raise ValueError(f"invalid promotion flag at ply {number}: {path}")
        for key in ("from_file", "from_rank", "to_file", "to_rank"):
            if type(move.get(key)) is not int or not 0 <= move[key] < 36:
                raise ValueError(f"invalid {key} at ply {number}: {path}")
        data = move.get("data")
        if data == "Standard":
            continue
        if not isinstance(data, dict) or len(data) != 1:
            raise ValueError(f"invalid move route at ply {number}: {path}")
        if "TwoStep" in data:
            route = data["TwoStep"]
            if not isinstance(route, dict) or any(
                type(route.get(key)) is not int or not 0 <= route[key] < 36
                for key in ("intermediate_file", "intermediate_rank")
            ):
                raise ValueError(f"invalid two-step route at ply {number}: {path}")
        elif "FreeEagle" in data:
            route = data["FreeEagle"]
            points = route.get("path") if isinstance(route, dict) else None
            if (
                not isinstance(points, list)
                or len(points) < 2
                or any(
                    not isinstance(point, dict)
                    or any(type(point.get(key)) is not int or not 0 <= point[key] < 36 for key in ("file", "rank"))
                    for point in points
                )
            ):
                raise ValueError(f"invalid Free Eagle route at ply {number}: {path}")
        else:
            raise ValueError(f"unknown move route at ply {number}: {path}")
    return True


def select_games(state, needed, roots_per_game, recent):
    done = [slot for slot in state["slots"] if slot.get("status") == "done"]
    if any(type(slot.get("id")) is not int or slot["id"] < 0 for slot in done):
        raise ValueError("completed slots need nonnegative integer IDs")
    if len({slot["id"] for slot in done}) != len(done):
        raise ValueError("duplicate completed slot IDs")
    pool = []
    skipped = []
    paths = set()
    hashes = set()
    for slot in sorted(done, key=lambda slot: slot["id"], reverse=True):
        if len(paths) >= recent:
            break
        if not slot.get("game_path"):
            raise ValueError(f"completed slot {slot['id']} has no game path")
        path = analysis.absolute(slot["game_path"]).resolve()
        if path in paths:
            continue
        paths.add(path)
        raw = path.read_bytes()
        record = load_json(raw)
        if not validate_record(record, path) or len(record["moves"]) < roots_per_game:
            skipped.append(dict(slot_id=slot["id"], path=str(path), reason="aborted, unfinished or too short"))
            continue
        sha = analysis.digest(raw)
        if sha in hashes:
            skipped.append(dict(slot_id=slot["id"], path=str(path), reason="duplicate complete record hash"))
            continue
        hashes.add(sha)
        pool.append(dict(slot_id=slot["id"], source_game=str(path), sha256=sha, record=record, raw=raw))
    pool.reverse()
    return [pool[i] for i in span_indices(len(pool), needed)], skipped


def root_plies(length, count, game_index):
    phases = [0, (length - 1) // 2, length - 1]
    rotated = phases[game_index % 3 :] + phases[: game_index % 3]
    choices = list(dict.fromkeys(rotated))
    choices += [ply for ply in span_indices(length, count) if ply not in choices]
    return sorted(choices[:count])


def prepare(run_dir, out, models=None, max_models=6, games=3, verify_games=2, roots_per_game=2, recent_games=30):
    if min(max_models, games, verify_games, roots_per_game, recent_games) < 1:
        raise ValueError("all corpus limits must be positive")
    if recent_games < games + verify_games:
        raise ValueError("--recent-games must cover training plus verification games")
    run = Path(run_dir).resolve()
    out = Path(out)
    if out.exists() or out.is_symlink():
        raise ValueError(f"output already exists; choose a new directory: {out}")
    out = out.resolve()
    state_path = run / "state.json"
    config_path = run / "analysis/config.json"
    state_data = state_path.read_bytes()
    config_data = config_path.read_bytes()
    state = load_json(state_data)
    config = load_json(config_data)
    selected_models = select_models(state, config, models, max_models)
    selected_games, skipped = select_games(state, games + verify_games, roots_per_game, recent_games)
    verification = {(i + 1) * len(selected_games) // (verify_games + 1) for i in range(verify_games)}
    corpora = {split: dict(positions=[], models=selected_models) for split in ("train", "verify")}
    evidence = []
    created = False
    try:
        out.mkdir(parents=True, exist_ok=False)
        created = True
        (out / "inputs").mkdir()
        (out / "inputs/state.json").write_bytes(state_data)
        (out / "inputs/config.json").write_bytes(config_data)
        for index, game in enumerate(selected_games):
            split = "verify" if index in verification else "train"
            directory = out / split / "games"
            directory.mkdir(parents=True, exist_ok=True)
            target = directory / (game["sha256"] + ".json")
            target.write_bytes(game["raw"])
            plies = root_plies(len(game["record"]["moves"]), roots_per_game, index)
            for ply in plies:
                corpora[split]["positions"].append(
                    dict(name=f"{split}-s{game['slot_id']}-p{ply}", game=str(target), ply=ply, sha256=game["sha256"])
                )
            evidence.append(
                dict(
                    split=split,
                    slot_id=game["slot_id"],
                    source_game=game["source_game"],
                    game=str(target),
                    game_id=game["record"]["game_id"],
                    sha256=game["sha256"],
                    total_moves=len(game["record"]["moves"]),
                    plies=plies,
                )
            )
        # Run state may advance while it is copied. Configuration/bindings must
        # not change underneath model selection; ask the caller to retry.
        if config_path.read_bytes() != config_data:
            raise ValueError("run model configuration changed during preparation; retry")
        for game in selected_games:
            if analysis.file_digest(game["source_game"]) != game["sha256"]:
                raise ValueError(f"completed game changed during preparation: {game['source_game']}")
        for item in selected_models:
            if analysis.file_digest(item["source_model"]) != item["source_sha256"]:
                raise ValueError(f"original model changed during preparation: {item['source_model']}")
            if analysis.file_digest(item["path"]) != item["sha256"]:
                raise ValueError(f"snapshot model changed during preparation: {item['path']}")
        selection = dict(
            format_version=1,
            run_dir=str(run),
            inputs=[
                dict(path=str(state_path), snapshot=str(out / "inputs/state.json"), sha256=analysis.digest(state_data)),
                dict(
                    path=str(config_path), snapshot=str(out / "inputs/config.json"), sha256=analysis.digest(config_data)
                ),
            ],
            limits=dict(
                max_models=max_models,
                games=games,
                verify_games=verify_games,
                roots_per_game=roots_per_game,
                recent_games=recent_games,
            ),
            policy="highest-slot-ID pool; deterministic span; interleaved heldout games; rotating early/middle/late roots",
            models=selected_models,
            games=evidence,
            skipped_games=skipped,
            validation="content hashes and record structure; compiled replay of selected prefixes and chosen full-route legality checked by profiling workload",
        )
        for split, corpus in corpora.items():
            filename = "corpus.json" if split == "train" else "verify-corpus.json"
            analysis.atomic(out / filename, corpus)
        analysis.atomic(out / "selection.json", selection)
        analysis.atomic(
            out / "profile-args.json",
            {
                split: dict(positions=list(range(len(corpus["positions"]))), models=list(range(len(selected_models))))
                for split, corpus in corpora.items()
            },
        )
        return selection
    except Exception:
        if created:
            shutil.rmtree(out, ignore_errors=True)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run-dir", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path, help="new directory; never overwrite an existing corpus")
    parser.add_argument(
        "--models", nargs="+", metavar="ENTRANT_ID", help="explicit active entrant IDs, without engine overrides"
    )
    parser.add_argument("--max-models", type=int, default=6)
    parser.add_argument("--games", type=int, default=3, help="training games")
    parser.add_argument("--verify-games", type=int, default=2, help="heldout games, never listed in training corpus")
    parser.add_argument("--roots-per-game", type=int, default=2)
    parser.add_argument(
        "--recent-games", type=int, default=30, help="maximum distinct highest-slot-ID game paths inspected"
    )
    args = parser.parse_args()
    try:
        selection = prepare(**vars(args))
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise SystemExit(f"PGO corpus preparation failed: {error}; no tournament was started or updated.") from error
    print(json.dumps(dict(out=str(args.out.resolve()), models=len(selection["models"]), games=len(selection["games"]))))


if __name__ == "__main__":
    main()
