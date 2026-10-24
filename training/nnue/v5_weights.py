"""Shared v5 preparation: score 512v4 errors once, preserve group weight totals."""

import argparse
import json
import os
import time
from collections import defaultdict
from pathlib import Path

import torch

import dataset_storage
from difficulty_weights import multiplier
from pilot_data import digest
from train import atomic_json, batch
from v4_data import Features

STRATA = ("representative", "precursor", "mate")


def mixture_weights(samples, weights, proportions):
    if len(proportions) != 3 or any(not 0 < p < 1 for p in proportions) or abs(sum(proportions) - 1) > 1e-8:
        raise ValueError("Three positive stratum proportions must sum to one")
    totals = defaultdict(float)
    for i, w in weights.items():
        totals[samples[i]["stratum"]] += w
    if set(totals) != set(STRATA):
        raise ValueError("Training mixture requires all three strata")
    mass = sum(weights.values())
    desired = dict(zip(STRATA, proportions, strict=True))
    return {i: w * desired[samples[i]["stratum"]] * mass / totals[samples[i]["stratum"]] for i, w in weights.items()}


def main():
    p = argparse.ArgumentParser()
    for name in ("source", "parent", "out", "cpu-file"):
        p.add_argument("--" + name, type=Path, required=True)
    a = p.parse_args()
    with dataset_storage.prepare(a.source):
        return prepare(a)


def prepare(a):
    from pilot_fit import from_quantized

    a.out.mkdir(parents=True, exist_ok=True)
    identity = json.loads((a.source / "dataset.json").read_text())
    schema = json.loads((a.source / "schema.json").read_text())
    parent = json.loads(a.parent.read_text())
    desc = parent["weights"]["nnue"]
    blob = a.parent.parent / desc["file"]
    if digest(blob) != desc["sha256"] or desc["feature_hash"] != schema["names_hash"]:
        raise ValueError("Parent weights/schema changed")
    binding = dict(
        source_sha256=digest(a.source / "dataset.json"),
        parent_sha256=digest(a.parent),
        parent_weights_sha256=desc["sha256"],
        code_sha256=digest(__file__),
        curve="smoothstep-035",
    )
    binding_path = a.out / "binding.json"
    if binding_path.exists() and json.loads(binding_path.read_text()) != binding:
        raise ValueError("Preparation inputs changed")
    atomic_json(binding_path, binding)
    if (a.out / "dataset.json").exists():
        saved = json.loads((a.out / "dataset.json").read_text())
        if digest(a.out / "samples.jsonl") != saved["samples_sha256"]:
            raise ValueError("Prepared samples changed")
        return
    if digest(a.source / "samples.jsonl") != identity["samples_sha256"]:
        raise ValueError("Source samples changed")
    samples = []
    cells = []
    for s in dataset_storage.rows(a.source / "samples.jsonl"):
        if s["split"] != "train":
            continue
        samples.append(
            {k: s[k] for k in ("offset", "us", "them", "score", "material", "target_probability", "sampling_weight")}
        )
        cells.append(tuple(s[k] for k in ("stratum", "group", "game", "episode")))
    torch.set_num_threads(2)
    torch.set_num_interop_threads(1)
    data = Features(a.source)
    net = from_quantized(blob, schema["features"])
    net.eval()
    cache = a.out / "difficulty.jsonl"
    scores = {}
    if cache.exists():
        for line in cache.open():
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                continue
            scores[r["index"]] = r
        cache.write_text("".join(json.dumps(r) + "\n" for r in scores.values()))
    missing = [i for i in range(len(samples)) if i not in scores]
    started = time.monotonic()
    with cache.open("a") as f, torch.no_grad():
        for start in range(0, len(missing), 8):
            if start % 800 == 0:
                cpus = set(json.loads(a.cpu_file.read_text())["cpus"])
                if not cpus:
                    raise ValueError("Empty training CPU lease")
                for task in Path("/proc/self/task").iterdir():
                    try:
                        os.sched_setaffinity(int(task.name), cpus)
                    except ProcessLookupError:
                        pass
                torch.set_num_threads(min(2, len(cpus)))
            group = missing[start : start + 8]
            x, off, _ = batch(samples, data, group)
            material = torch.tensor([samples[i]["material"] for i in group])
            probabilities = torch.sigmoid((net(x, off) * 1000 + material) / identity["scale"]).tolist()
            for i, prob in zip(group, probabilities, strict=True):
                error = abs(prob - samples[i]["target_probability"])
                r = dict(index=i, probability=prob, error=error, multiplier=multiplier(error))
                scores[i] = r
                f.write(json.dumps(r) + "\n")
            if start % 800 == 0:
                f.flush()
                atomic_json(
                    a.out / "status.json",
                    dict(state="scoring", done=len(scores), total=len(samples), elapsed_s=time.monotonic() - started),
                )
    totals = defaultdict(lambda: [0.0, 0.0])
    for i, s in enumerate(samples):
        totals[cells[i]][0] += s["sampling_weight"]
        totals[cells[i]][1] += s["sampling_weight"] * scores[i]["multiplier"]
    import struct

    train_index = 0
    count = 0
    overrides = a.out / "sample-overrides.bin"
    temporary = overrides.with_suffix(".tmp")
    with temporary.open("wb") as f:
        for s in dataset_storage.rows(a.source / "samples.jsonl"):
            weight, error, boost = s.get("sampling_weight", 1.0), 0.0, 1.0
            if s["split"] == "train":
                r = scores[train_index]
                original, boosted = totals[cells[train_index]]
                weight *= r["multiplier"] * original / boosted
                error, boost = r["error"], r["multiplier"]
                train_index += 1
            f.write(struct.pack("<ddd", weight, error, boost))
            count += 1
    temporary.replace(overrides)
    atomic_json(
        a.out / "sample-view.json",
        dict(
            version=1,
            source=str((a.source / "samples.jsonl").resolve()),
            source_sha256=digest(a.source / "samples.jsonl"),
            overrides=overrides.name,
            overrides_sha256=digest(overrides),
            samples=count,
        ),
    )
    (a.out / "schema.json").write_bytes((a.source / "schema.json").read_bytes())
    identity.update(policy="v5-common-smooth-v1", weighting=binding, samples_sha256=digest(a.out / "samples.jsonl"))
    atomic_json(a.out / "dataset.json", identity)
    atomic_json(a.out / "status.json", dict(state="completed", training_samples=len(samples)))


if __name__ == "__main__":
    main()
