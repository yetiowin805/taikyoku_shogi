#!/usr/bin/env python3
"""Prepare a portable snapshot workload using matching, already-local weights."""

import argparse
import copy
import hashlib
import json
from pathlib import Path


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(workload, out):
    corpus = json.loads(workload.read_text())
    out.mkdir(parents=True, exist_ok=False)
    for position in corpus["positions"]:
        path = Path(position["game"]).resolve()
        if sha(path) != position["sha256"]:
            raise ValueError(f"snapshot hash mismatch: {path}")
        position["game"] = str(path)
    for model in corpus["models"]:
        base_path = Path(model["local_base"]).resolve()
        checkpoint = json.loads(base_path.read_text())
        weights = copy.deepcopy(checkpoint["weights"])
        descriptor = weights.pop("nnue")
        identity = hashlib.sha256(json.dumps(weights, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        if identity != model["non_nnue_weights_sha256"]:
            raise ValueError(f"material weights mismatch: {base_path}")
        network = (base_path.parent / descriptor["file"]).resolve()
        if descriptor["sha256"] != model["nnue_sha256"] or sha(network) != model["nnue_sha256"]:
            raise ValueError(f"NNUE mismatch: {network}")
        checkpoint["name"] = model["name"]
        checkpoint["search_defaults"] = model["search_defaults"]
        checkpoint["weights"]["nnue"]["file"] = str(network)
        path = out / f"{model['name']}.json"
        path.write_text(json.dumps(checkpoint) + "\n")
        model["path"] = str(path.resolve())
        model["sha256"] = sha(path)
    path = out / "corpus.json"
    path.write_text(json.dumps(corpus, indent=2) + "\n")
    return path


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("workload", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    print(prepare(args.workload, args.out))
