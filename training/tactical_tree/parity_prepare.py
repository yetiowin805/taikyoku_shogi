"""Freeze a small stage-stratified sample before examining any new parity results."""

import argparse
import hashlib
import json
import random
import shutil
from pathlib import Path

from leader import model_inputs

from run import atomic, sha


def select_cases(run, excluded, seed):
    state = json.loads((run / "state.json").read_text())
    candidates = [
        s
        for s in state["slots"]
        if s["status"] == "done"
        and s["id"] >= 1300
        and s["id"] not in excluded
        and any("_v4" in s[k] or "_v5" in s[k] for k in ("model_a", "model_b"))
    ]
    candidates.sort(key=lambda s: hashlib.sha256(f"{seed}:{s['id']}".encode()).hexdigest())
    rng = random.Random(seed)
    phases = [("early", 0.05, 0.25), ("middle", 0.35, 0.65), ("late", 0.75, 0.95)] * 4
    cases = []
    for slot in candidates:
        game = json.loads(Path(slot["game_path"]).read_text())
        if len(game["moves"]) < 100:
            continue
        phase, lo, hi = phases[len(cases)]
        index = rng.randrange(int(len(game["moves"]) * lo), int(len(game["moves"]) * hi))
        cases.append(
            dict(
                id=f"s{slot['id']:04}-p{index + 1:04}",
                slot=slot["id"],
                start_ply=index + 1,
                game=slot["game_path"],
                cohort="representative",
                phase=phase,
                start_color=game["moves"][index]["color"],
                plies=len(game["moves"]),
                maximum_depth=5 if len(cases) < 3 else 4,
            )
        )
        if len(cases) == 12:
            break
    if len(cases) != 12:
        raise ValueError("not enough eligible distinct completed games")
    return cases


def prepare(args):
    if args.config.exists():
        raise ValueError("config already exists; resume it or choose a new run")
    args.out.mkdir(parents=True, exist_ok=True)
    previous = json.loads(args.previous.read_text())
    excluded = {c["slot"] for c in previous["cases"]} | {672, 769, 867}
    cases = select_cases(args.run, excluded, args.seed)
    for slot, ply in [(1337, 1024), (673, 174), (673, 176)]:
        old = next(c for c in previous["cases"] if c["slot"] == slot)
        game = json.loads(Path(old["game"]).read_text())
        cases.append(
            dict(
                id=f"s{slot:04}-p{ply:04}",
                slot=slot,
                start_ply=ply,
                game=old["game"],
                cohort="diagnostic",
                phase="known_parity_or_tactical",
                start_color=game["moves"][ply - 1]["color"],
                plies=len(game["moves"]),
                maximum_depth=5,
            )
        )
    frozen = args.out / "inputs"
    frozen.mkdir(exist_ok=True)
    for case in cases:
        source = Path(case["game"])
        dest = frozen / source.name
        shutil.copyfile(source, dest)
        case.update(game=str(dest.resolve()), source_game=str(source), sha256=sha(dest))
    entrants = json.loads((args.run / "analysis/manifest.json").read_text())["entrants"]
    models = {}
    defaults = None
    for entrant in entrants:
        if not entrant["id"].startswith("NNUE_") or not any(x in entrant["id"] for x in ("_v4", "_v5")):
            continue
        manifest, paths = model_inputs(entrant["model"])
        if defaults is not None and defaults != manifest["search_defaults"]:
            raise ValueError("models have different search settings; explicitly resolve before launch")
        defaults = manifest["search_defaults"]
        manifest["weights"]["nnue"]["file"] = str(paths[1])
        dest = frozen / (entrant["id"] + ".json")
        atomic(dest, manifest)
        models[entrant["id"]] = dict(
            path=str(dest.resolve()),
            source=entrant["model"],
            source_sha256=sha(paths[0]),
            width=manifest["weights"]["nnue"]["width"],
            identities={str(dest.resolve()): sha(dest), str(paths[1]): sha(paths[1])},
        )
    if not {"NNUE_W512_v4", "NNUE_W512_v5a", "NNUE_W512_v5b", "NNUE_W512_v5c"} <= models.keys():
        raise ValueError("required comparison checkpoints missing")
    binary = args.binary.resolve()
    sources = list(Path(__file__).parent.glob("*.py")) + [binary]
    config = dict(
        version=1,
        experiment="parity-v1",
        seed=args.seed,
        binary=str(binary),
        binary_sha256=sha(binary),
        models=models,
        cases=cases,
        out=str(args.out.resolve()),
        workers=2,
        lease_indices=[4, 5],
        cpus=list(range(8)),
        control=str((args.run / "analysis").resolve()),
        admission_status=args.admission_status,
        minimum_free_bytes=4 * 1024**3,
        identities={str(p.resolve()): sha(p) for p in sources},
        build=json.loads(args.build.read_text()),
        selection="12 distinct recent NNUE v4/v5 games, hash-selected; 4 per relative game phase; no score-based selection. Three known diagnostics separate. First position in each phase and all diagnostics preselected for depth five.",
        limitation="Small purposive frame, not a population prevalence estimate. Standalone q probes lack the actual leaf's previous-move context. Original immutable NNUE blobs referenced by absolute path and checked by hash; private inputs are not uploaded to GitHub.",
    )
    atomic(args.config, config)
    print(
        json.dumps(dict(config=str(args.config), cases=len(cases), models=list(models), jobs=len(cases) * len(models)))
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--previous", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--binary", type=Path, required=True)
    p.add_argument("--build", type=Path, required=True)
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--admission-status", required=True)
    p.add_argument("--seed", type=int, default=20261019)
    prepare(p.parse_args())
