import hashlib
import json
import tempfile
import unittest
from collections import defaultdict
from pathlib import Path

from difficulty_weights import build, cell, multiplier


class DifficultyTests(unittest.TestCase):
    def test_smooth_monotone_bounded_and_continuous_at_old_thresholds(self):
        values = [multiplier(i / 1000) for i in range(1001)]
        self.assertEqual(values, sorted(values))
        self.assertEqual((values[0], values[-1]), (1, 2))
        self.assertEqual(multiplier(0.175), 1.5)
        self.assertEqual(multiplier(0.35), 2)
        for edge in (0.1, 0.25, 0.35):
            self.assertLess(abs(multiplier(edge + 1e-8) - multiplier(edge - 1e-8)), 1e-6)
        for bad in (True, None, -0.1, 1.1, float("nan"), float("inf")):
            with self.assertRaises(ValueError):
                multiplier(bad)

    def fixture(self, root):
        root.mkdir()
        rows = []
        for episode in ("a", "b"):
            for e, old, w in ((0.01, 1, 0.1), (0.17, 1.5, 0.3), (0.4, 2, 0.2)):
                rows.append(
                    dict(
                        split="train",
                        stratum="representative",
                        group="group",
                        game="game",
                        episode=episode,
                        parent_probability_error=e,
                        difficulty_multiplier=old,
                        sampling_weight=w,
                        target_probability=0.5,
                        material=10,
                        offset=5,
                    )
                )
        rows.extend(dict(split=split, sampling_weight=0.5, target_probability=0.7) for split in ("validation", "test"))
        data = "".join(json.dumps(r) + "\n" for r in rows).encode()
        (root / "samples.jsonl").write_bytes(data)
        (root / "schema.json").write_bytes(b"{}")
        (root / "dataset.json").write_text(
            json.dumps(
                dict(
                    samples=len(rows),
                    samples_sha256=hashlib.sha256(data).hexdigest(),
                    schema_sha256=hashlib.sha256(b"{}").hexdigest(),
                    feature_shards=[dict(path="features.bin", sha256="unchanged")],
                    weighting=dict(parent_sha256="frozen-parent"),
                )
            )
        )
        return rows, data

    def test_normalization_replaces_boost_holdouts_unchanged_and_idempotent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            rows, original = self.fixture(root / "source")
            build(root / "source", root / "first")
            actual = [json.loads(s) for s in (root / "first/samples.jsonl").read_text().splitlines()]
            self.assertEqual(actual[-2:], rows[-2:])
            self.assertEqual((root / "source/samples.jsonl").read_bytes(), original)
            totals = defaultdict(float)
            for r in actual[:-2]:
                totals[cell(r)] += r["sampling_weight"]
                self.assertEqual(r["target_probability"], 0.5)
                self.assertEqual(r["offset"], 5)
            self.assertTrue(all(abs(w - 0.6) < 1e-12 for w in totals.values()))
            # Relative weighting removes the old 1.5x instead of compounding it.
            expected_ratio = (0.3 / 1.5 * multiplier(0.17)) / (0.1 * multiplier(0.01))
            self.assertAlmostEqual(actual[1]["sampling_weight"] / actual[0]["sampling_weight"], expected_ratio)
            build(root / "first", root / "second")
            again = [json.loads(s) for s in (root / "second/samples.jsonl").read_text().splitlines()]
            for a, b in zip(actual, again, strict=True):
                self.assertAlmostEqual(a["sampling_weight"], b["sampling_weight"])
            identity = json.loads((root / "first/dataset.json").read_text())
            self.assertEqual(identity["weighting"]["parent_sha256"], "frozen-parent")
            self.assertEqual(identity["feature_shards"][0]["path"], str(root / "source/features.bin"))
            with self.assertRaises(ValueError):
                build(root / "source", root / "first")

    def test_invalid_source_cannot_publish_dataset(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self.fixture(root / "source")
            with (root / "source/samples.jsonl").open("ab") as f:
                f.write(b"{}\n")
            with self.assertRaises((ValueError, KeyError)):
                build(root / "source", root / "out")
            self.assertFalse((root / "out/dataset.json").exists())


if __name__ == "__main__":
    unittest.main()
