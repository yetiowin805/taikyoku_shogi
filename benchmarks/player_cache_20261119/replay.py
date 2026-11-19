"""Compare fresh processes on six consecutive fixed-depth requests per game and pairing."""

import argparse
import hashlib
import json
import random
import resource
import subprocess
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
PAIRS = {"384": (2, 1), "512": (2, 0), "1536": (2, 3)}


def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()


def child_cpu():
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    return usage.ru_utime + usage.ru_stime


def execute(exe, corpus, requests, cpu):
    before = child_cpu()
    start = time.monotonic()
    replies = []
    with subprocess.Popen(
        ["taskset", "-c", str(cpu), str(exe), str(corpus)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    ) as p:
        for q in requests:
            p.stdin.write(json.dumps(q) + "\n")
            p.stdin.flush()
            r = json.loads(p.stdout.readline())
            assert r["nnue"] and not r["aborted"] and r["completed_depth"] == q["depth"]
            replies.append(r)
        with open(f"/proc/{p.pid}/smaps_rollup") as f:
            pss = next(int(x.split()[1]) for x in f if x.startswith("Pss:"))
        p.stdin.close()
        if p.wait() != 0:
            raise RuntimeError("search server failed")
    return dict(wall=time.monotonic() - start, cpu=child_cpu() - before, pss_kib=pss, replies=replies)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets", type=Path, required=True)
    parser.add_argument("--arm", action="append", required=True)
    parser.add_argument("--cpu", type=int, default=6)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--isolation", action="store_true", help="compare each side alone with interleaved cache counters"
    )
    args = parser.parse_args()
    assets = args.assets.resolve()
    arms = {k: Path(v).resolve() for k, v in (a.split("=", 1) for a in args.arm)}
    manifest = json.loads((HERE / "inputs.json").read_text())
    models = []
    for m in manifest["models"]:
        path = assets / m["file"]
        assert sha(path) == m["sha256"]
        descriptor = json.loads(path.read_text())["weights"]["nnue"]
        assert sha(path.parent / descriptor["file"]) == m["blob_sha256"]
        models.append(dict(path=str(path), name=m["name"]))
    pairs = dict(PAIRS)
    if args.isolation:
        pairs["identical"] = (2, 2)
    games = manifest["games"][:2] if args.isolation else manifest["games"]
    for game in games:
        assert sha(assets / game["file"]) == game["sha256"]
    rng = random.Random(20261119)
    groups = [(name, i) for i in range(len(games)) for name in pairs]
    rng.shuffle(groups)
    with tempfile.TemporaryDirectory() as tmp, args.out.open("x") as f:
        print(json.dumps(dict(builds={k: sha(v) for k, v in arms.items()}, cpu=args.cpu, seed=20261119)), file=f)
        for name, i in groups:
            g = games[i]
            positions = [
                dict(name=f"slot{g['slot']}-{j}", game=str(assets / g["file"]), ply=g["first"] + j) for j in range(6)
            ]
            corpus = Path(tmp) / "corpus.json"
            corpus.write_text(json.dumps(dict(positions=positions, models=models)))
            qs = [dict(position=j, depth=2, model=pairs[name][(i + j) % 2]) for j in range(6)]
            order = list(arms)
            rng.shuffle(order)
            baseline = None
            for rep in range(1 if args.isolation else 2):
                for arm in order if rep == 0 else reversed(order):
                    r = execute(arms[arm], corpus, qs, args.cpu)
                    sig = [x["signature"] for x in r["replies"]]
                    if baseline is None:
                        baseline = sig
                    assert sig == baseline
                    if args.isolation:
                        for side in range(2):
                            alone = execute(arms[arm], corpus, qs[side::2], args.cpu)
                            for match, solo in zip(r["replies"][side::2], alone["replies"], strict=True):
                                assert match["signature"] == solo["signature"]
                                assert match["cache_counts"] == solo["cache_counts"]
                    print(json.dumps(dict(pair=name, slot=g["slot"], rep=rep, arm=arm, **r)), file=f, flush=True)
            print(name, g["slot"], flush=True)


if __name__ == "__main__":
    main()
