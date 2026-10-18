"""Freeze input identities and validate a pilot configuration before any CPU is borrowed."""

import argparse
import json
import os
from pathlib import Path

from leader import model_inputs, select_leader

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
    inputs = [args.corpus, args.binary, *Path(__file__).parent.glob("*.py")]
    model = args.model.resolve(strict=True) if args.model else None
    if model:
        _, paths = model_inputs(model)
        inputs.extend(paths)
    if args.leader_run:
        state = json.loads((args.leader_run / "state.json").read_text())
        fit = json.loads((args.leader_run / "order-neutral-ratings.json").read_text())
        entrant, _ = select_leader(state, fit)
        model_inputs(entrant["model"])  # Fail before taking leases if the current leader is unavailable.
    identities = {str(p.resolve()): sha(p) for p in inputs}
    identities.update({c["game"]: c["sha256"] for c in cases})
    if not args.control.is_dir() or not os.access(args.binary, os.X_OK):
        raise ValueError("missing coordinator control directory or executable; pilot did not start")
    if args.config.exists():
        raise ValueError("configuration already exists; choose a fresh file or explicitly reuse it")
    config = dict(
        version=1,
        binary=str(args.binary.resolve()),
        model=str(model) if model else None,
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
    if args.comparison:
        if (
            not args.checkpoints
            or any(p <= 0 for p in args.checkpoints)
            or sorted(set(args.checkpoints)) != args.checkpoints
        ):
            raise ValueError("checkpoints must be positive, distinct, and increasing")
        config.update(
            experiment="matched-analysis-v1",
            checkpoints=args.checkpoints,
            leader_run=str(args.leader_run.resolve()) if args.leader_run else None,
        )
    args.out.mkdir(parents=True, exist_ok=True)
    atomic(args.config, config)
    print(
        json.dumps(
            dict(
                config=str(args.config),
                cases=len(cases),
                estimated_cpu_hours=len(cases)
                * (3 * max(args.checkpoints) + 1500 if args.comparison else 4 * 420 + 540)
                / 3600,
            )
        )
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("corpus", "binary", "out", "control", "admission-status", "config", "build"):
        parser.add_argument("--" + name, type=Path, required=True)
    models = parser.add_mutually_exclusive_group(required=True)
    models.add_argument("--model", type=Path)
    models.add_argument("--leader-run", type=Path)
    parser.add_argument("--comparison", action="store_true")
    parser.add_argument("--checkpoints", type=int, nargs="+", default=[60, 180, 420])
    parser.add_argument("--game-dir", type=Path)
    arguments = parser.parse_args()
    if arguments.leader_run and not arguments.comparison:
        parser.error("--leader-run requires --comparison")
    try:
        prepare(arguments)
    except (OSError, ValueError, KeyError) as exc:
        parser.exit(1, f"prepare: {exc}; pilot did not start\n")
