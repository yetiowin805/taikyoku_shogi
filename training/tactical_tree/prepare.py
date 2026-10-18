"""Freeze input identities and validate a pilot configuration before any CPU is borrowed."""

import argparse
import json
import os
from pathlib import Path

from run import atomic, sha


def prepare(args):
    corpus = json.loads(args.corpus.read_text())
    cases = []
    for source in corpus["cases"]:
        case = dict(source)
        game = (args.game_dir / case["game_filename"]) if args.game_dir else Path(case["game"])
        game = game.resolve(strict=True)
        if sha(game) != case["sha256"]:
            raise ValueError("frozen game changed: " + str(game))
        case["game"] = str(game)
        cases.append(case)
    model = args.model.resolve(strict=True)
    manifest = json.loads(model.read_text())
    inputs = [args.corpus, args.binary, model, *Path(__file__).parent.glob("*.py")]
    network = manifest.get("weights", {}).get("nnue")
    if network:
        blob = model.parent / network["file"]
        if sha(blob) != network["sha256"]:
            raise ValueError("NNUE content hash mismatch")
        inputs.append(blob)
    identities = {str(p.resolve()): sha(p) for p in inputs}
    identities.update({c["game"]: c["sha256"] for c in cases})
    if not args.control.is_dir() or not os.access(args.binary, os.X_OK):
        raise ValueError("missing coordinator control directory or executable; pilot did not start")
    if args.config.exists():
        raise ValueError("configuration already exists; choose a fresh file or explicitly reuse it")
    config = dict(
        version=1,
        binary=str(args.binary.resolve()),
        model=str(model),
        out=str(args.out.resolve()),
        control=str(args.control.resolve()),
        admission_status=str(args.admission_status.resolve()),
        cases=cases,
        identities=identities,
        case_seconds=420,
        workers=2,
        lease_indices=[4, 5, 6, 7],
        cpus=list(range(8)),
        minimum_free_bytes=4 * 1024**3,
        seed=20261017,
        build=json.loads(args.build.read_text()),
    )
    args.out.mkdir(parents=True, exist_ok=True)
    atomic(args.config, config)
    print(
        json.dumps(
            dict(config=str(args.config), cases=len(cases), estimated_cpu_hours=len(cases) * (4 * 420 + 540) / 3600)
        )
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("corpus", "binary", "model", "out", "control", "admission-status", "config", "build"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--game-dir", type=Path)
    arguments = parser.parse_args()
    try:
        prepare(arguments)
    except (OSError, ValueError, KeyError) as exc:
        parser.exit(1, f"prepare: {exc}; pilot did not start\n")
