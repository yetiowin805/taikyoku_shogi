"""Bind each episode to the current rating leader, then keep all its arms comparable."""

import json
import math
import shutil
import time
from pathlib import Path

from run import atomic, sha, validate_inputs


def model_inputs(model):
    model = Path(model).resolve(strict=True)
    manifest = json.loads(model.read_text())
    paths = [model]
    network = manifest.get("weights", {}).get("nnue")
    if network:
        blob = (model.parent / network["file"]).resolve(strict=True)
        if sha(blob) != network["sha256"]:
            raise ValueError("NNUE content hash mismatch: " + str(blob))
        paths.append(blob)
    return manifest, paths


def select_leader(state, fit):
    if fit.get("status") not in ("converged", "separated", "disconnected"):
        raise ValueError("no usable order-neutral rating fit")
    active = {e["id"]: e for e in state["entrants"] if e["id"] not in state.get("retired", [])}
    ratings = fit.get("ratings", {})
    if not ratings or any(type(v) not in (int, float) or not math.isfinite(v) for v in ratings.values()):
        raise ValueError("invalid leader ratings")
    eligible = active.keys() & ratings.keys()
    if not eligible:
        raise ValueError("no active rated model")
    # Follow the displayed ranking, including provisional component offsets. No promotion margin.
    name = min(eligible, key=lambda n: (-ratings[n], n))
    return active[name], ratings[name]


def bind_case(config, case):
    """Called by the single supervisor before dispatch; resume never rebinds an episode."""
    out = Path(config["out"]) / case["id"]
    out.mkdir(parents=True, exist_ok=True)
    path = out / "config.json"
    if path.exists():
        bound = json.loads(path.read_text())
        if bound["parent_config"] != config or bound["bound_case"] != case:
            raise ValueError("episode configuration changed")
        validate_inputs(bound)
        return path
    bound = dict(config, identities=dict(config["identities"]), parent_config=config, bound_case=case)
    if config.get("leader_run"):
        run = Path(config["leader_run"])
        state = json.loads((run / "state.json").read_text())
        fit = json.loads((run / "order-neutral-ratings.json").read_text())
        entrant, rating = select_leader(state, fit)
        source = Path(entrant["model"])
        manifest, inputs = model_inputs(source)
        if manifest.get("name") != entrant["id"]:
            raise ValueError("leader checkpoint name differs from entrant")
        digest = sha(source)
        frozen = Path(config["out"]) / "models" / digest
        frozen.mkdir(parents=True, exist_ok=True)
        if len(inputs) == 2:
            # Keep the frozen copy self-contained even if the original uses an absolute blob path.
            manifest["weights"]["nnue"]["file"] = inputs[1].name
            blob = frozen / inputs[1].name
            if not blob.exists():
                shutil.copyfile(inputs[1], blob.with_suffix(".tmp"))
                blob.with_suffix(".tmp").replace(blob)
        model = frozen / "model.json"
        if not model.exists():
            atomic(model, manifest)
        elif json.loads(model.read_text()) != manifest:
            raise ValueError("frozen leader manifest changed")
        _, paths = model_inputs(model)
        bound["model"] = str(model)
        bound["identities"].update({str(p): sha(p) for p in paths})
        bound["leader"] = dict(
            id=entrant["id"],
            rating=rating,
            selected_at=time.time(),
            source=str(source),
            source_sha256=digest,
            fit=fit,
        )
    atomic(path, bound)
    return path
