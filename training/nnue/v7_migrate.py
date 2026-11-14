"""Stream v1 residual checkpoints into v2; preserve dense layers and fixed material."""

import argparse
import hashlib
import json
import os
import struct
from pathlib import Path


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            value.update(chunk)
    return value.hexdigest()


def atomic(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def migrate(parent_path, old, new, output):
    parent_path, output = Path(parent_path), Path(output)
    parent = json.loads(parent_path.read_text())
    descriptor = parent["weights"]["nnue"]
    source = parent_path.parent / descriptor["file"]
    if old["version"] != 1 or new["version"] != 2:
        raise ValueError("migration requires feature versions 1 -> 2")
    for schema in (old, new):
        if schema["features"] != 2592 * schema["channels"] or len(schema["names"]) > schema["channels"]:
            raise ValueError("invalid feature dimensions")
        if len(set(schema["names"])) != len(schema["names"]):
            raise ValueError("duplicate canonical feature names")
        names_hash = hashlib.sha256(json.dumps(schema["names"], separators=(",", ":")).encode()).hexdigest()
        if names_hash != schema["names_hash"]:
            raise ValueError("feature names hash mismatch")
    if descriptor.get("output_mode", "residual") != "residual":
        raise ValueError("v7 retains fixed material; parent must be residual")
    if descriptor["feature_hash"] != old["names_hash"] or digest(source) != descriptor["sha256"]:
        raise ValueError("parent content or feature identity mismatch")
    identity = dict(parent_sha256=digest(parent_path), old_schema=old["hash"], new_schema=new["hash"])
    marker = output / "migration.json"
    if marker.exists():
        saved = json.loads(marker.read_text())
        if saved["identity"] != identity or digest(output / saved["network"]) != saved["network_sha256"]:
            raise ValueError("migration output changed or belongs to another parent")
        if digest(output / "model.json") != saved["model_sha256"]:
            raise ValueError("migrated model metadata changed")
        return saved
    output.mkdir(parents=True, exist_ok=True)
    width = descriptor["width"]
    # Old jump channels mixed reflection conventions. Do not transplant those
    # ambiguous learned rows; shared ordinary movement rows retain their meanings.
    shared = {name: i for i, name in enumerate(old["names"]) if not name.startswith(("jump:", "knight:"))}
    copied = [(shared[name], i) for i, name in enumerate(new["names"]) if name in shared]
    temporary = output / "network.tmp"
    row_bytes = 2 * width
    dense_bytes = width * 4 + 2 * width * 32 + 32 * 4 + 32 * 32 + 32 * 4 + 33 * 4
    with source.open("rb") as src, temporary.open("wb") as dst:
        header = src.read(64)
        if len(header) != 64 or header[:8] != b"TKNNUE01":
            raise ValueError("invalid checkpoint header")
        if struct.unpack("<III", header[8:20]) != (1, width, old["channels"]):
            raise ValueError("parent header dimensions mismatch")
        if header[20:52].hex() != old["hash"] or struct.unpack("<III", header[52:64]) != (4096, 64, 1):
            raise ValueError("parent feature schema, quantization or material mode mismatch")
        dst.write(
            header[:8] + struct.pack("<III", 2, width, new["channels"]) + bytes.fromhex(new["hash"]) + header[52:]
        )
        for _ in range(2592):
            previous = src.read(old["channels"] * row_bytes)
            if len(previous) != old["channels"] * row_bytes:
                raise ValueError("truncated parent embedding")
            current = bytearray(new["channels"] * row_bytes)
            for before, after in copied:
                current[after * row_bytes : (after + 1) * row_bytes] = previous[
                    before * row_bytes : (before + 1) * row_bytes
                ]
            dst.write(current)
        dense = src.read(dense_bytes)
        if len(dense) != dense_bytes or src.read(1):
            raise ValueError("invalid parent checkpoint length")
        dst.write(dense)
        dst.flush()
        os.fsync(dst.fileno())
    network_hash = digest(temporary)
    target = output / ("nnue-" + network_hash + ".bin")
    temporary.replace(target)
    parent["name"] = output.name
    parent["weights"]["nnue"] = dict(
        descriptor, file=target.name, sha256=network_hash, feature_hash=new["names_hash"], output_mode="residual"
    )
    atomic(output / "model.json", parent)
    result = dict(
        identity=identity,
        network=target.name,
        network_sha256=network_hash,
        model_sha256=digest(output / "model.json"),
        width=width,
        copied_channels=[new["names"][i] for _, i in copied],
        zero_channels=[name for name in new["names"] if name not in shared],
        dense_layers="byte-identical",
        fixed_material="unchanged",
    )
    atomic(marker, result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("parent", type=Path)
    parser.add_argument("old_schema", type=Path)
    parser.add_argument("new_schema", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    result = migrate(
        args.parent, json.loads(args.old_schema.read_text()), json.loads(args.new_schema.read_text()), args.output
    )
    print(json.dumps({k: result[k] for k in ("width", "network_sha256", "dense_layers", "fixed_material")}))


if __name__ == "__main__":
    main()
