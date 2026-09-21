#!/usr/bin/env python3
"""Short six-way leave-one-out study on the merged search baseline.

Run from the source worktree after building both native binaries. All model and
game inputs come from the frozen earlier corpus; no live tournament access.
"""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import time


VARIANTS = {
    "main": ("baseline", ""),
    "full": ("both", "packed,royal"),
    "no-cache": ("snapshot", "packed,royal"),
    "no-snapshot": ("fused", "packed,royal"),
    "no-packed": ("both", "royal"),
    "no-royal": ("both", "packed"),
}
PARITY = ["depth", "score", "static_score", "nodes", "qnodes", "best", "root_lines"]


def sha(path):
    with open(path, "rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--root", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--stock", type=Path, required=True)
    p.add_argument("--probes", type=Path, required=True)
    p.add_argument("--phase", choices=["verify", "timed"], required=True)
    p.add_argument("--reps", type=int, default=2)
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    positions = json.loads((a.root / "data/nnue-speed-probe-20260920/manifest.json").read_text())["positions"]
    models = {w: a.root / f"data/nnue-admission-v2/NNUE_W{w}_v2.json" for w in [512, 2048]}
    seed = 2026092101
    rng = random.Random(seed)
    # Balance order by reversing each block in repetition 2, then shuffle blocks.
    blocks = [(w, pos, rng.sample(list(VARIANTS), len(VARIANTS))) for w in models for pos in positions]
    order = []
    for rep in range(a.reps if a.phase == "timed" else 1):
        shuffled = rng.sample(blocks, len(blocks))
        for width, pos, variants in shuffled:
            for variant in (variants if rep % 2 == 0 else variants[::-1]):
                order.append(dict(rep=rep, width=width, position=pos, variant=variant))
    source_files = subprocess.check_output(["git", "ls-files", "src", "Cargo.toml", "Cargo.lock"], text=True).splitlines()
    source_files += ["examples/nnue_speed_probe.rs", __file__]
    manifest = {
        "created": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        "main_revision": subprocess.check_output(["git", "rev-parse", "origin/main"], text=True).strip(),
        "working_diff": subprocess.check_output(["git", "diff"], text=True),
        "source_sha256": {f: sha(f) for f in source_files},
        "phase": a.phase, "seed": seed, "variants": VARIANTS, "order": order,
        "cpu": 0, "time_ms": 3000, "timed_depth_ceiling": 8, "verify_depth": 2,
        "warmup": "one untimed depth-1 search in each process; TT storage then reused",
        "binary_sha256": {"stock": sha(a.stock), "probes": sha(a.probes)},
        "game_sha256": {p["game"]: sha(p["game"]) for p in positions},
        "models": {str(w): {"descriptor_sha256": sha(path), "nnue": json.loads(path.read_text())["weights"]["nnue"]} for w, path in models.items()},
        "compiler": subprocess.check_output(["rustc", "-Vv"], text=True),
        "hardware": subprocess.check_output(["lscpu"], text=True),
        "rustflags": "-C target-cpu=native", "profile": "release; thin LTO; codegen-units=1",
    }
    with (a.out / f"{a.phase}-manifest.json").open("x") as f:
        json.dump(manifest, f, indent=2)
    rows = []
    with (a.out / f"{a.phase}.jsonl").open("x") as f:
        for i, job in enumerate(order):
            pos, width, variant = job["position"], job["width"], job["variant"]
            env = {k: v for k, v in os.environ.items() if not k.startswith("NNUE_")}
            env.update(NNUE_SPEED_PROBE=VARIANTS[variant][0], NNUE_FOLLOWUP=VARIANTS[variant][1], NNUE_ABLATION_WARMUP="1")
            binary = a.stock if variant == "main" else a.probes
            limit, depth = (30000, 2) if a.phase == "verify" else (3000, 8)
            cmd = ["/usr/bin/time", "-f", "PEAK_RSS_KIB=%M", "taskset", "-c", "0", str(binary), str(models[width]), pos["game"], str(pos["ply"]), str(limit), str(depth), "search"]
            start = time.monotonic()
            run = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=90)
            (a.out / f"{a.phase}-{i:03d}.stderr").write_text(run.stderr)
            if run.returncode:
                raise RuntimeError(run.stderr)
            result = json.loads(run.stdout)
            assert result["warmup"] and result["probes_compiled"] == (variant != "main"), job
            assert result["legal"], job
            if a.phase == "verify":
                assert not result["aborted"] and result["depth"] == depth, job
            row = dict(job, position=pos["name"], game=pos["game"], ply=pos["ply"], result=result,
                       wall_seconds=time.monotonic() - start,
                       peak_rss_kib=int(run.stderr.split("PEAK_RSS_KIB=")[-1].strip()))
            f.write(json.dumps(row) + "\n")
            f.flush()
            rows.append(row)
            print(f'{a.phase} {i+1}/{len(order)} W{width} {pos["name"]} {variant}: depth {result["depth"]}, {result["nodes"] / (result["elapsed_ns"] / 1e9):.0f} nps', flush=True)
    if a.phase == "verify":
        for row in rows:
            base = next(b for b in rows if b["width"] == row["width"] and b["position"] == row["position"] and b["variant"] == "main")
            for key in PARITY:
                assert row["result"][key] == base["result"][key], (row["width"], row["position"], row["variant"], key)
        print("All six variants preserve fixed-depth scores, routes, root lines and node counts.", flush=True)


if __name__ == "__main__":
    main()
