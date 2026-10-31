"""Shared sample views and explicit hydration before mmap-based training."""

import contextlib
import json
import struct
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "deploy"))
import archive_storage
import artifact_storage


def hydrate(path):
    path = Path(path)
    archive = archive_storage.configured()
    if archive:
        archive.hydrate(path)
    return path


def rows(path):
    """Legacy JSONL or a bound overlay, preserving order and float64 values."""
    path = Path(path)
    if path.name == "samples.jsonl" and (path.parent / "sample-view.json").exists():
        view = json.loads((path.parent / "sample-view.json").read_text())
        source = Path(view["source"])
        if sample_digest(source) != view["source_sha256"]:
            raise ValueError("Overlay source changed")
        weights = hydrate(path.parent / view["overrides"])
        if archive_storage.digest(weights) != view["overrides_sha256"]:
            raise ValueError("Overlay overrides changed")
        count = 0
        with weights.open("rb") as overlay:
            for sample in rows(source):
                data = overlay.read(24)
                if len(data) != 24:
                    raise ValueError("Truncated sample overlay")
                weight, error, multiplier = struct.unpack("<ddd", data)
                if sample["split"] == "train":
                    sample.update(
                        sampling_weight=weight, parent_probability_error=error, difficulty_multiplier=multiplier
                    )
                yield sample
                count += 1
            if overlay.read(1) or count != view["samples"]:
                raise ValueError("Sample overlay count mismatch")
    else:
        with hydrate(path).open() as source:
            for line in source:
                if line.strip():
                    yield json.loads(line)


def sample_digest(path):
    path = Path(path)
    view = path.parent / "sample-view.json"
    if path.name == "samples.jsonl" and view.exists():
        return archive_storage.digest(view)
    return archive_storage.digest(hydrate(path))


def inputs(root, seen=None):
    root = Path(root).resolve()
    seen = set() if seen is None else seen
    if root in seen:
        raise ValueError("Cyclic sample view")
    seen.add(root)
    identity = json.loads((root / "dataset.json").read_text())
    paths = [root / "schema.json"]
    view = root / "sample-view.json"
    if view.exists():
        spec = json.loads(view.read_text())
        paths.extend(inputs(Path(spec["source"]).parent, seen))
        paths.extend([view, root / spec["overrides"]])
    else:
        paths.append(root / "samples.jsonl")
    paths.extend(Path(s["path"]) for s in identity.get("feature_shards", []))
    if "feature_shards" not in identity:
        paths.append(root / "features.bin")
    return paths


def prepare(root, out=None, width=None, checkpoint_dir=None, extra_inputs=()):
    """Hold this lease for the whole consumer lifetime, including queued jobs."""
    archive = archive_storage.configured()
    if archive is None:
        return contextlib.nullcontext()
    paths = inputs(root)
    for path in paths:
        if path.name == "schema.json":
            archive.register_dataset(path.parent)
    requirements = {}
    if out is not None and width is not None:
        schema = json.loads((Path(root) / "schema.json").read_text())
        # Two complete checkpoint generations with current+best float32 tables,
        # plus dense optimizer state and atomic model publication. Includes both
        # output filesystems when optimizer state lives on a separate disk.
        table = schema["features"] * width
        requirements[Path(checkpoint_dir or out)] = table * 16 + 16 * 1024**2
        requirements[Path(out)] = requirements.get(Path(out), 0) + table * 4 + 16 * 1024**2
    return archive.pin([*paths, *extra_inputs], requirements, mutable_paths=extra_inputs)


def share(source, target=None, sha=None):
    source = Path(source)
    artifact_storage.snapshot_blob(source, Path(target or source), sha or archive_storage.digest(source))


def archive_checkpoint(path):
    archive = archive_storage.configured()
    if archive and archive.config.get("archive_completed_checkpoints") and Path(path).exists():
        archive.archive([path], "closed-training")
        archive.evict([path])


def share_game(source, target, sha):
    artifact_storage.frozen_game(source, target, sha)


if __name__ == "__main__":
    import argparse
    import subprocess
    import time

    parser = argparse.ArgumentParser(description="Hydrate and pin inputs while running a legacy training command")
    parser.add_argument("dataset", type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("A training command is required after --")
    with prepare(args.dataset) as lease:
        child = subprocess.Popen(command, start_new_session=True)
        try:
            while child.poll() is None:
                if lease:
                    lease.check_headroom()
                time.sleep(1)
            if child.returncode:
                raise subprocess.CalledProcessError(child.returncode, command)
        finally:
            if child.poll() is None:
                import os
                import signal

                os.killpg(child.pid, signal.SIGTERM)
                child.wait()
