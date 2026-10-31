"""PGO input selection is deterministic, content-pinned and read-only to a run."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import prepare_pgo_corpus as corpus


class CorpusTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.run = self.root / "run"
        self.control = self.run / "analysis/models"
        self.control.mkdir(parents=True)
        self.originals = self.root / "models"
        self.originals.mkdir()
        self.blob = self.originals / "shared.nnue"
        self.blob.write_bytes(b"structural fixture, not a loadable neural network")
        self.frozen_blob = self.control / self.blob.name
        self.frozen_blob.write_bytes(self.blob.read_bytes())
        self.state = dict(entrants=[], retired=[], slots=[])
        self.config = dict(models={})
        self.add_model("HC")
        self.add_model("N512", 512)
        self.add_model("N512-policy", 512, "royal-safety")
        self.add_model("N768", 768)
        self.add_model("N1024", 1024)
        self.add_model("N1536", 1536)
        self.add_model("N2048", 2048)
        self.add_model("alias", alias="N512")
        self.add_model("retired", retired=True)
        self.add_model("historical", engine="/historical/engine")
        self.records = []
        for index in range(8):
            record = dict(
                format_version=2,
                game_id=f"game-{index}",
                seed=index,
                start=dict(kind="opening"),
                black=dict(name="ab", model=str(self.originals / "HC.json")),
                white=dict(name="ab", model=str(self.originals / "N512.json")),
                moves=[
                    dict(
                        move_number=i + 1,
                        color="Black" if i % 2 == 0 else "White",
                        from_file=i,
                        from_rank=10,
                        to_file=i,
                        to_rank=11,
                        promoted=False,
                        data="Standard",
                    )
                    for i in range(12 + index)
                ],
                result="Draw",
            )
            path = self.run / f"game-{index}.json"
            self.write(path, record)
            self.records.append(path)
            self.state["slots"].append(dict(id=index, status="done", game_path=str(path)))
        self.state["slots"].append(dict(id=100, status="running", game_path="must-not-read"))
        self.flush()

    @staticmethod
    def write(path, value):
        path.write_text(json.dumps(value, sort_keys=True) + "\n")

    def add_model(self, name, width=None, policy="baseline", alias=None, retired=False, engine=None, trial="none"):
        source = self.originals / f"{name}.json"
        if alias:
            source.write_bytes((self.originals / f"{alias}.json").read_bytes())
        else:
            checkpoint = dict(name=name, weights={}, search_defaults=dict(experiment=policy))
            if trial != "none":
                checkpoint["search_defaults"]["trial"] = trial
            if width:
                checkpoint["weights"]["nnue"] = dict(
                    width=width, file=self.blob.name, sha256=corpus.analysis.file_digest(self.blob)
                )
            self.write(source, checkpoint)
        raw = source.read_bytes()
        checkpoint = json.loads(raw)
        binding = dict(sha256=corpus.analysis.digest(raw))
        target = self.control / (binding["sha256"] + ".json")
        binding["snapshot"] = str(target)
        if descriptor := checkpoint["weights"].get("nnue"):
            descriptor["file"] = str(self.frozen_blob)
            binding["artifacts"] = [
                dict(original=str(self.blob), snapshot=str(self.frozen_blob), sha256=descriptor["sha256"])
            ]
        self.write(target, checkpoint)
        binding["snapshot_sha256"] = corpus.analysis.file_digest(target)
        self.config["models"][str(source)] = binding
        self.state["entrants"].append(dict(id=name, model=str(source), engine=engine))
        if retired:
            self.state["retired"].append(name)

    def flush(self):
        self.write(self.run / "state.json", self.state)
        self.write(self.run / "analysis/config.json", self.config)

    def test_bounded_disjoint_full_records_and_unchanged_run(self):
        before = {path: path.read_bytes() for path in (self.run / "state.json", self.run / "analysis/config.json")}
        out = self.root / "corpus"
        selected = corpus.prepare(self.run, out)
        train = json.loads((out / "corpus.json").read_text())
        verify = json.loads((out / "verify-corpus.json").read_text())
        self.assertEqual(len(train["positions"]), 6)
        self.assertEqual(len(verify["positions"]), 4)
        self.assertEqual(len(train["models"]), 6)
        self.assertEqual(train["models"], verify["models"])
        names = {model["name"] for model in train["models"]}
        self.assertTrue({"HC", "N2048"} <= names)
        self.assertFalse({"retired", "historical"} & names)
        self.assertTrue({p["sha256"] for p in train["positions"]}.isdisjoint(p["sha256"] for p in verify["positions"]))
        for game in selected["games"]:
            original = Path(game["source_game"]).read_bytes()
            self.assertEqual(Path(game["game"]).read_bytes(), original)
            self.assertEqual(corpus.analysis.digest(original), game["sha256"])
            self.assertEqual(len(json.loads(original)["moves"]), game["total_moves"])
            self.assertEqual(len(set(game["plies"])), 2)
            self.assertTrue(all(0 <= ply < game["total_moves"] for ply in game["plies"]))
            self.assertIn(f"/{game['split']}/games/", game["game"])
        for path, raw in before.items():
            self.assertEqual(path.read_bytes(), raw)
        self.assertEqual((out / "inputs/state.json").read_bytes(), before[self.run / "state.json"])
        again = corpus.prepare(self.run, self.root / "again")

        def identity(selection):
            return [(game["source_game"], game["sha256"], game["plies"], game["split"]) for game in selection["games"]]

        self.assertEqual(identity(selected), identity(again))

    def test_exact_checkpoint_dedup_retains_shared_blob_distinct_policies(self):
        selected = corpus.prepare(self.run, self.root / "out", models=["alias", "N512", "N512-policy"])
        models = selected["models"]
        self.assertEqual([model["name"] for model in models], ["alias", "N512-policy"])
        self.assertEqual(models[0]["aliases"], ["alias", "N512"])
        self.assertEqual(models[0]["blob_sha256"], models[1]["blob_sha256"])
        self.assertNotEqual(models[0]["source_sha256"], models[1]["source_sha256"])
        self.assertEqual(models[1]["search_defaults"]["experiment"], "royal-safety")

    def test_g3_trials_have_distinct_default_coverage_groups(self):
        trials = {"G3A": "internal_pvs", "G3B": "q_history", "G3C": "context_retry"}
        for name, trial in trials.items():
            self.add_model(name, 512, "history_tactical_lmr_q_fix", trial=trial)
        self.state["entrants"] = [entry for entry in self.state["entrants"] if entry["id"] in trials]
        self.flush()
        selected = corpus.prepare(self.run, self.root / "out", max_models=2)["models"]
        # With one width/foundation, the bounded span still samples distinct
        # trial groups rather than taking a single group plus arbitrary extras.
        self.assertEqual({model["search_defaults"]["trial"] for model in selected}, {"context_retry", "q_history"})
        self.assertEqual(len({tuple(model["selection_group"]) for model in selected}), 2)
        self.assertEqual(len({model["blob_sha256"] for model in selected}), 1)
        explicit = corpus.prepare(self.run, self.root / "explicit", models=["G3A", "G3B"])["models"]
        self.assertEqual([model["name"] for model in explicit], ["G3A", "G3B"])
        self.assertNotEqual(explicit[0]["source_sha256"], explicit[1]["source_sha256"])

    def test_every_resolved_binding_alias_keeps_its_pin(self):
        source = self.originals / "N512.json"
        alias = self.originals / "linked.json"
        alias.symlink_to(source)
        self.config["models"][str(alias)] = dict(self.config["models"][str(source)])
        self.flush()
        corpus.prepare(self.run, self.root / "good", models=["N512"])
        self.config["models"][str(alias)]["sha256"] = "0" * 64
        self.flush()
        with self.assertRaisesRegex(ValueError, "original model changed"):
            corpus.prepare(self.run, self.root / "bad", models=["N512"])
        self.assertFalse((self.root / "bad").exists())

    def test_unselected_artifacts_are_not_hashed(self):
        self.blob.write_bytes(b"unselected network changed")
        selected = corpus.prepare(self.run, self.root / "out", models=["HC"])
        self.assertEqual([item["name"] for item in selected["models"]], ["HC"])

    def test_explicit_model_errors_and_existing_output_do_not_overwrite(self):
        for requested in [["unknown"], ["retired"], ["historical"], ["HC", "HC"]]:
            with self.subTest(requested=requested), self.assertRaises(ValueError):
                corpus.prepare(self.run, self.root / "bad", models=requested)
            self.assertFalse((self.root / "bad").exists())
        with self.assertRaisesRegex(ValueError, "exceeds"):
            corpus.prepare(self.run, self.root / "bad", models=["HC", "N512"], max_models=1)
        existing = self.root / "existing"
        existing.mkdir()
        marker = existing / "keep"
        marker.write_text("untouched")
        with self.assertRaisesRegex(ValueError, "already exists"):
            corpus.prepare(self.run, existing)
        self.assertEqual(marker.read_text(), "untouched")

    def test_original_snapshot_and_both_blob_copies_are_content_pinned(self):
        source = self.originals / "N512.json"
        snapshot = Path(self.config["models"][str(source)]["snapshot"])
        for path in [source, snapshot, self.blob, self.frozen_blob]:
            raw = path.read_bytes()
            path.write_bytes(b"changed")
            with self.subTest(path=path), self.assertRaisesRegex(ValueError, "changed"):
                corpus.prepare(self.run, self.root / "bad", models=["N512"])
            self.assertFalse((self.root / "bad").exists())
            path.write_bytes(raw)

    def test_corrupt_completed_record_is_not_silently_skipped(self):
        path = self.records[-1]
        record = json.loads(path.read_text())
        cases = [
            dict(record, format_version=3),
            dict(record, moves=[dict(record["moves"][0], to_rank=36)]),
            dict(
                record,
                moves=[dict(record["moves"][0], data=dict(TwoStep=dict(intermediate_file=-1, intermediate_rank=0)))],
            ),
            dict(record, moves=[dict(record["moves"][0], data=dict(FreeEagle=dict(path=[dict(file=0, rank=0)])))]),
            dict(record, moves=[dict(record["moves"][0], move_number=True)]),
            dict(record, moves=[dict(record["moves"][0], move_number=0)]),
            dict(record, moves=[dict(record["moves"][0], move_number=2**64)]),
        ]
        for value in cases:
            self.write(path, value)
            with self.subTest(value=value), self.assertRaises(ValueError):
                corpus.prepare(self.run, self.root / "bad")
            self.assertFalse((self.root / "bad").exists())
        path.write_text('{"format_version":2,"seed":NaN}')
        with self.assertRaisesRegex(ValueError, "nonfinite"):
            corpus.prepare(self.run, self.root / "bad")

    def test_compound_move_numbers_do_not_change_prefix_roots_or_record_bytes(self):
        path = self.records[-1]
        record = json.loads(path.read_text())
        record["moves"][0]["data"] = dict(TwoStep=dict(intermediate_file=0, intermediate_rank=12))
        for index, move in enumerate(record["moves"]):
            move["move_number"] = 1 if index == 0 else index + 2
        self.write(path, record)
        raw = path.read_bytes()
        selected = corpus.prepare(self.run, self.root / "out", games=1, verify_games=1, recent_games=2)
        copied = next(game for game in selected["games"] if game["source_game"] == str(path))
        self.assertEqual(Path(copied["game"]).read_bytes(), raw)
        self.assertTrue(all(0 <= ply < len(record["moves"]) for ply in copied["plies"]))
        self.assertEqual([move["move_number"] for move in record["moves"][:3]], [1, 3, 4])

    def test_structural_validation_leaves_turn_sequence_to_engine_replay(self):
        record = json.loads(self.records[-1].read_text())
        record["moves"][1]["color"] = record["moves"][0]["color"]
        self.assertTrue(corpus.validate_record(record, self.records[-1]))
        record["moves"][1]["color"] = "invalid"
        with self.assertRaisesRegex(ValueError, "move color"):
            corpus.validate_record(record, self.records[-1])

    def test_finished_game_filter_duplicate_hashes_and_insufficient_pool(self):
        aborted = json.loads(self.records[-1].read_text())
        aborted["abort_reason"] = "worker stopped"
        self.write(self.records[-1], aborted)
        unfinished = json.loads(self.records[-2].read_text())
        unfinished["result"] = None
        self.write(self.records[-2], unfinished)
        duplicate = self.run / "duplicate.json"
        duplicate.write_bytes(self.records[-3].read_bytes())
        self.state["slots"].append(dict(id=9, status="done", game_path=str(duplicate)))
        self.flush()
        selected = corpus.prepare(self.run, self.root / "out")
        self.assertEqual(len(selected["games"]), 5)
        self.assertEqual(len({game["sha256"] for game in selected["games"]}), 5)
        self.assertEqual(len(selected["skipped_games"]), 3)
        with self.assertRaisesRegex(ValueError, "distinct inputs"):
            corpus.prepare(self.run, self.root / "too-many", games=5, verify_games=2)
        self.assertFalse((self.root / "too-many").exists())

    def test_live_config_change_and_record_change_remove_partial_output(self):
        original_select = corpus.select_games

        def changed_config(*args):
            result = original_select(*args)
            with (self.run / "analysis/config.json").open("a") as stream:
                stream.write(" ")
            return result

        with (
            patch.object(corpus, "select_games", side_effect=changed_config),
            self.assertRaisesRegex(ValueError, "configuration changed"),
        ):
            corpus.prepare(self.run, self.root / "bad")
        self.assertFalse((self.root / "bad").exists())
        self.flush()

        def changed_record(*args):
            result = original_select(*args)
            Path(result[0][0]["source_game"]).write_text("changed")
            return result

        with (
            patch.object(corpus, "select_games", side_effect=changed_record),
            self.assertRaisesRegex(ValueError, "game changed"),
        ):
            corpus.prepare(self.run, self.root / "bad")
        self.assertFalse((self.root / "bad").exists())

    def test_root_selection_never_uses_terminal_ply_and_covers_phases(self):
        self.assertEqual(corpus.root_plies(20, 2, 0), [0, 9])
        self.assertEqual(corpus.root_plies(20, 2, 1), [9, 19])
        self.assertEqual(corpus.root_plies(20, 2, 2), [0, 19])
        for length in range(1, 12):
            for count in range(1, length + 1):
                for index in range(3):
                    roots = corpus.root_plies(length, count, index)
                    self.assertEqual(len(set(roots)), count)
                    self.assertTrue(all(0 <= ply < length for ply in roots))


if __name__ == "__main__":
    unittest.main()
