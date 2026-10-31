"""Smooth cached parent-error weights into a new immutable dataset; no inference."""

import argparse
import hashlib
import json
import math
from collections import defaultdict
from pathlib import Path

POLICY = "parent-error-smoothstep-035-v1"


def multiplier(error):
    if (
        isinstance(error, bool)
        or not isinstance(error, (int, float))
        or not math.isfinite(error)
        or not 0 <= error <= 1
    ):
        raise ValueError("parent probability error must be finite and in [0, 1]")
    t = min(error / 0.35, 1.0)
    return 1 + t * t * (3 - 2 * t)


def cell(row):
    return tuple(row[key] for key in ("stratum", "group", "game", "episode"))


def terms(row):
    weight = row["sampling_weight"]
    prior = row.get("difficulty_multiplier", 1.0)
    if (
        isinstance(weight, bool)
        or not isinstance(weight, (int, float))
        or not math.isfinite(weight)
        or weight < 0
        or isinstance(prior, bool)
        or not isinstance(prior, (int, float))
        or not math.isfinite(prior)
        or not 1 <= prior <= 2
    ):
        raise ValueError("invalid sampling weight or prior difficulty multiplier")
    boost = multiplier(row["parent_probability_error"])
    return weight, weight / prior * boost, boost


def build(source, out):
    source, out = Path(source).resolve(), Path(out).resolve()
    if out.exists():
        raise ValueError("output must be a new directory; existing datasets are immutable")
    identity_bytes = (source / "dataset.json").read_bytes()
    identity = json.loads(identity_bytes)
    if "feature_shards" not in identity:
        raise ValueError("expected a sharded dataset")
    schema = (source / "schema.json").read_bytes()
    if hashlib.sha256(schema).hexdigest() != identity["schema_sha256"]:
        raise ValueError("schema hash mismatch")
    totals = defaultdict(lambda: [0.0, 0.0])
    digest = hashlib.sha256()
    count = train = 0
    with (source / "samples.jsonl").open("rb") as src:
        for line in src:
            digest.update(line)
            row = json.loads(line)
            count += 1
            if row["split"] not in ("train", "validation", "test"):
                raise ValueError("unknown sample split")
            if row["split"] == "train":
                weight, provisional, _ = terms(row)
                sums = totals[cell(row)]
                sums[0] += weight
                sums[1] += provisional
                train += 1
    if digest.hexdigest() != identity["samples_sha256"] or count != identity["samples"]:
        raise ValueError("source samples hash/count mismatch")
    # Two streaming passes avoid loading the feature shards or full samples in RAM.
    out.mkdir(parents=True)
    output_hash, second_hash = hashlib.sha256(), hashlib.sha256()
    with (out / "samples.jsonl").open("wb") as dest, (source / "samples.jsonl").open("rb") as src:
        for line in src:
            second_hash.update(line)
            row = json.loads(line)
            if row["split"] == "train":
                _, provisional, boost = terms(row)
                total, boosted = totals[cell(row)]
                row.update(
                    sampling_weight=provisional * total / boosted if boosted else 0.0,
                    difficulty_multiplier=boost,
                    difficulty_policy=POLICY,
                )
                line = (json.dumps(row) + "\n").encode()
            dest.write(line)
            output_hash.update(line)
    if second_hash.digest() != digest.digest() or (source / "dataset.json").read_bytes() != identity_bytes:
        raise ValueError("source changed during conversion; output is incomplete (no dataset.json)")
    (out / "schema.json").write_bytes(schema)
    for shard in identity["feature_shards"]:
        p = Path(shard["path"])
        shard["path"] = str((source / p).resolve())
    identity.update(
        policy=POLICY,
        samples_sha256=output_hash.hexdigest(),
        difficulty_smoothing=dict(
            policy=POLICY,
            source=str(source),
            source_dataset_sha256=hashlib.sha256(identity_bytes).hexdigest(),
            source_samples_sha256=digest.hexdigest(),
            code_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            training_samples=train,
            cells=len(totals),
            cap=2,
            saturation_error=0.35,
        ),
    )
    # Publish the manifest last. Feature/target/holdout data are unchanged.
    manifest = out / "dataset.json.tmp"
    manifest.write_text(json.dumps(identity, indent=2) + "\n")
    manifest.replace(out / "dataset.json")
    return identity["difficulty_smoothing"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.source, args.out), indent=2))


if __name__ == "__main__":
    main()
