"""Repeat the W512/W512 direct comparison with order balanced within every move."""

import argparse
import contextlib
import json
import random
import subprocess
import tempfile
from pathlib import Path

from replay import HERE, sha


def main():
    p = argparse.ArgumentParser(description=__doc__)
    for key in ["assets", "bounded", "owned", "out"]:
        p.add_argument("--" + key, type=Path, required=True)
    p.add_argument("--cpu", type=int, default=6)
    a = p.parse_args()
    manifest = json.loads((HERE / "inputs.json").read_text())
    models = []
    for m in manifest["models"][:3]:
        path = (a.assets / m["file"]).resolve()
        assert sha(path) == m["sha256"]
        desc = json.loads(path.read_text())["weights"]["nnue"]
        assert sha(path.parent / desc["file"]) == m["blob_sha256"]
        models.append(dict(name=m["name"], path=str(path)))
    arms = dict(bounded=a.bounded, owned=a.owned, control=a.owned)
    rng = random.Random(191126)
    sequence = list(enumerate(manifest["games"]))
    rng.shuffle(sequence)
    with tempfile.TemporaryDirectory() as tmp, a.out.open("x") as out:
        for i, g in sequence:
            path = (a.assets / g["file"]).resolve()
            assert sha(path) == g["sha256"]
            pos = [dict(game=str(path), name=str(step), ply=g["first"] + step) for step in range(6)]
            corpus = Path(tmp) / "corpus.json"
            corpus.write_text(json.dumps(dict(models=models, positions=pos)))
            orders = [rng.sample(list(arms), len(arms)) for _ in pos]
            for rep in range(2):
                with contextlib.ExitStack() as stack:
                    ps = {
                        name: stack.enter_context(
                            subprocess.Popen(
                                ["taskset", "-c", str(a.cpu), str(exe), str(corpus)],
                                stdin=subprocess.PIPE,
                                stdout=subprocess.PIPE,
                                text=True,
                            )
                        )
                        for name, exe in arms.items()
                    }
                    for step, order in enumerate(orders):
                        request = dict(position=step, model=(2, 0)[(i + step) % 2], depth=2)
                        outputs = {}
                        for k in order if rep == 0 else reversed(order):
                            proc = ps[k]
                            proc.stdin.write(json.dumps(request) + "\n")
                            proc.stdin.flush()
                            outputs[k] = json.loads(proc.stdout.readline())
                        assert len({r["signature"] for r in outputs.values()}) == 1
                        assert all(not r["aborted"] and r["completed_depth"] == 2 for r in outputs.values())
                        print(
                            json.dumps(dict(slot=g["slot"], step=step, rep=rep, measurements=outputs)),
                            file=out,
                            flush=True,
                        )
                    for proc in ps.values():
                        proc.stdin.close()
                        assert proc.wait() == 0


if __name__ == "__main__":
    main()
