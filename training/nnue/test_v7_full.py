"""Full-v7 migration, split preservation and experiment-isolation regressions."""

import hashlib
import json
import struct
import tempfile
import unittest
from pathlib import Path

import v6_pipeline
import v7_full
import v7_full_data
import v7_migrate


class FullV7Tests(unittest.TestCase):
    def test_restart_retries_archival_before_releasing_dependents(self):
        job = dict(name="prelude", fit=dict(full_v7=True, prelude=True))
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            out = root / "prelude"
            out.mkdir()
            (out / "start.pt").write_bytes(b"saved prelude")
            complete = out / "complete.json"
            complete.write_text("{}")
            self.assertFalse(v6_pipeline.finished(root, job))
            (out / "storage-complete.json").write_text(json.dumps(dict(complete_sha256=v7_migrate.digest(complete))))
            self.assertTrue(v6_pipeline.finished(root, job))
            complete.write_text('{"changed":true}')
            with self.assertRaisesRegex(ValueError, "storage completion identity"):
                v6_pipeline.finished(root, job)

    def test_streaming_migration_preserves_shared_rows_dense_head_and_material(self):
        def schema(version, names):
            return dict(
                version=version,
                names=names,
                channels=len(names),
                features=2592 * len(names),
                hash=("01" if version == 1 else "02") * 32,
                names_hash=hashlib.sha256(json.dumps(names, separators=(",", ":")).encode()).hexdigest(),
            )

        old = schema(1, ["knight:sideways", "occupancy"])
        new = schema(2, ["identity:GoldGeneral", "occupancy", "promotion-available"])
        width = 32
        dense = bytes(width * 4 + 2 * width * 32 + 32 * 4 + 32 * 32 + 32 * 4 + 33 * 4)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "parent.bin"
            with source.open("wb") as stream:
                stream.write(
                    b"TKNNUE01"
                    + struct.pack("<III", 1, width, old["channels"])
                    + bytes.fromhex(old["hash"])
                    + struct.pack("<III", 4096, 64, 1)
                )
                for square in range(2592):
                    stream.write(struct.pack("<" + "h" * 64, *([7] * 32 + [square % 100] * 32)))
                stream.write(dense)
            material = dict(piece=[1, 2, 3])
            parent = dict(
                name="parent",
                weights=dict(
                    material,
                    nnue=dict(
                        file=source.name,
                        sha256=v7_migrate.digest(source),
                        width=width,
                        feature_hash=old["names_hash"],
                        output_mode="residual",
                    ),
                ),
            )
            (root / "parent.json").write_text(json.dumps(parent))
            target = root / "migrated"
            result = v7_migrate.migrate(root / "parent.json", old, new, target)
            with (target / result["network"]).open("rb") as stream:
                self.assertEqual(struct.unpack("<III", stream.read(64)[8:20]), (2, width, 3))
                for square in range(2592):
                    rows = struct.unpack("<" + "h" * 96, stream.read(192))
                    self.assertEqual(rows[:32], (0,) * 32)
                    self.assertEqual(rows[32:64], (square % 100,) * 32)
                    self.assertEqual(rows[64:], (0,) * 32)
                self.assertEqual(stream.read(), dense)
            migrated = json.loads((target / "model.json").read_text())
            self.assertEqual(migrated["weights"]["piece"], material["piece"])
            self.assertEqual(v7_migrate.migrate(root / "parent.json", old, new, target), result)
            (target / "model.json").write_text("{}")
            with self.assertRaisesRegex(ValueError, "metadata changed"):
                v7_migrate.migrate(root / "parent.json", old, new, target)

    def test_full_matrix_has_thirty_residual_arms_without_pilot_deadlines(self):
        jobs = v7_full.tasks(dict(out="/training", parents={"256": "/a", "384": "/b"}))
        arms = [j for j in jobs if not j["fit"].get("prelude")]
        self.assertEqual(len(arms), 30)
        self.assertEqual(len({j["name"] for j in jobs}), 32)
        self.assertEqual({j["fit"]["regime"] for j in arms}, {"A", "Ao", "C", "Cu", "E"})
        for job in arms:
            self.assertTrue(job["fit"]["retain_material"])
            self.assertTrue(job["fit"]["full_v7"])
            self.assertNotIn("pilot_windows", job["fit"])
            self.assertNotIn("deadline", job["fit"])

    def test_group_cap_and_recent_fresh_test_preserve_inherited_splits(self):
        rows = {}
        for group in range(20):
            for game in range(6):
                identity = f"{group:02}-{game}"
                rows[identity] = dict(
                    id=identity,
                    group=f"{group:02}",
                    split="train",
                    inherited=False,
                    timestamp=1000 + group,
                    quarantine=False,
                )
        rows["old-test"] = dict(id="old-test", group="old-test", split="test", inherited=True, quarantine=False)
        rows["pilot-validation"] = dict(
            id="pilot-validation", group="pilot-validation", split="validation", inherited=True, quarantine=False
        )
        config = dict(seed=10, representative=8, challenge=2, roots_per_group=40, minimum_fresh_groups=10)
        selected = v7_full_data.select_games(rows, config)
        self.assertEqual(len(selected), 20 * 4 + 2)
        self.assertEqual(selected["old-test"]["split"], "test")
        self.assertEqual(selected["pilot-validation"]["split"], "validation")
        self.assertEqual(
            {r["group"] for r in selected.values() if r["split"] == "test" and not r["inherited"]}, {"18", "19"}
        )
        for group in range(20):
            self.assertEqual(len({r["split"] for r in selected.values() if r["group"] == f"{group:02}"}), 1)

    def test_holdout_audit_never_uses_error_ranking(self):
        rows = [
            dict(id=f"{split}-{i}", game_id=f"game-{i}", split=split)
            for split in ("train", "validation", "test")
            for i in range(20)
        ]

        def priority(row):
            self.assertEqual(row["split"], "train")
            return 0

        picked = v7_full_data.audit_selection(rows, 0.10, 11, priority)
        self.assertEqual(len(picked), 6)
        for split in ("train", "validation", "test"):
            self.assertEqual(sum(r["split"] == split for r in picked), 2)


if __name__ == "__main__":
    unittest.main()
