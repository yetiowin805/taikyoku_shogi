"""Time the production tournament game child on fixed-depth, capped self-play segments."""

import argparse
import hashlib
import json
import random
import subprocess
import tempfile
import time
from pathlib import Path

from replay import HERE, child_cpu, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for name in ("repo", "assets", "stock", "owned", "out"):
        p.add_argument("--" + name, type=Path, required=True)
    p.add_argument("--cpu", type=int, default=4)
    p.add_argument("--limit", type=int, default=12, help="tournament move-number cap; two-step moves count twice")
    a = p.parse_args()
    manifest = json.loads((HERE / "inputs.json").read_text())
    archive = a.repo / "benchmarks/non_search_speed_20261102"
    sources = json.loads((archive / "workload.json").read_text())["positions"]
    rng = random.Random(182619)
    cases = list(range(6))
    rng.shuffle(cases)
    builds = {k: sha(getattr(a, k)) for k in ("stock", "owned")}
    with tempfile.TemporaryDirectory() as tmp, a.out.open("x") as output:
        out = Path(tmp)
        for i in cases:
            snapshot = archive / "positions" / f"position-{i}.json"
            start = json.loads(snapshot.read_text())["start"]
            # Each width pairing occurs once per color assignment.
            pair = [2, (1, 0, 3)[i % 3]]
            if i >= 3:
                pair.reverse()
            agents = []
            for mid in pair:
                m = manifest["models"][mid]
                model = (a.assets / m["file"]).resolve()
                assert sha(model) == m["sha256"]
                descriptor = json.loads(model.read_text())
                assert descriptor.get("search_defaults", {}).get("max_time_ms") is None
                agents.append(dict(name="ab", model=str(model), depth=2))
            order = list(builds)
            rng.shuffle(order)
            reference = None
            for rep in range(2):
                for arm in order if rep == 0 else reversed(order):
                    exe = getattr(a, arm).resolve()
                    bundle = dict(
                        revision=arm,
                        protocol=1,
                        engine_bin=str(exe),
                        engine_sha256=builds[arm],
                        analyzer_bin=str(exe),
                        analyzer_sha256=builds[arm],
                    )
                    bound = [dict(agent, engine_sha256=builds[arm], engine_build=bundle) for agent in agents]
                    config = dict(
                        black=bound[0], white=bound[1], start=start, seed=42, max_moves=a.limit, verbose=False
                    )
                    job = out / "job.json"
                    job.write_text(json.dumps(dict(protocol=1, bundle=bundle, config=config)))
                    dest = out / "record.json"
                    before = child_cpu()
                    begin = time.monotonic()
                    subprocess.run(
                        ["taskset", "-c", str(a.cpu), str(exe), "tournament-game", str(job), str(dest)], check=True
                    )
                    wall = time.monotonic() - begin
                    cpu = child_cpu() - before
                    data = json.loads(dest.read_text())
                    for mv in data["moves"]:
                        assert mv["completed_depth"] == 2
                        mv.pop("iteration_timings", None)
                    key = [data["moves"], data["result"], data.get("abort_reason")]
                    sig = hashlib.sha256(json.dumps(key, sort_keys=True).encode()).hexdigest()
                    if reference is None:
                        reference = sig
                    assert reference == sig
                    print(
                        json.dumps(
                            dict(
                                position=i,
                                source_game=Path(sources[i]["source_game"]).name,
                                arm=arm,
                                rep=rep,
                                cpu=cpu,
                                wall=wall,
                                play_ms=data["stats"]["elapsed_ms"],
                                plies=len(data["moves"]),
                                signature=sig,
                                models=pair,
                                build_sha=builds[arm],
                                snapshot_sha=sha(snapshot),
                            )
                        ),
                        file=output,
                        flush=True,
                    )
            print("finished position", i, flush=True)


if __name__ == "__main__":
    main()
