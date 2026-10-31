import copy
import unittest

import label_storage
import training_labels as labels
from test_tourney_analysis import AnalyzerFixture, a


class SamplingTests(unittest.TestCase):
    def game(self):
        return dict(
            start={"kind": "opening"},
            result="BlackWins",
            black={"name": "ab"},
            white={"name": "ab"},
            moves=[
                dict(
                    color="Black" if i % 2 == 0 else "White",
                    eval=(i // 2 % 2) * 2000,
                    from_file=i % 36,
                    from_rank=3,
                    to_file=1,
                    to_rank=4,
                    data="Standard",
                    promoted=False,
                )
                for i in range(200)
            ],
        )

    def test_deterministic_bounded_stratified_and_disjoint(self):
        g = self.game()
        rows = labels.candidates(g)
        self.assertEqual(rows, labels.candidates(g))
        self.assertLessEqual(len(rows), 20)
        self.assertEqual(len({r["center_ply"] for r in rows}), len(rows))
        self.assertEqual(sum(r["selection"] == "representative" for r in rows), 12)
        self.assertEqual({r["selection"] for r in rows}, {"representative", "evaluation_swing"})
        self.assertTrue(all(r["plies"] == [r["center_ply"]] for r in rows))
        other = copy.deepcopy(g)
        other["moves"][0]["eval"] = 700
        other["black"]["model"] = "different"
        self.assertEqual(rows[0]["split_group"], labels.candidates(other)[0]["split_group"])
        other["moves"][0]["to_file"] = 2
        self.assertNotEqual(rows[0]["split_group"], labels.candidates(other)[0]["split_group"])

    def test_bad_scores_draws_historical_and_short_games(self):
        g = self.game()
        g["result"] = "Draw"
        g["moves"][0]["eval"] = True
        g["moves"][1]["eval"] = float("nan")
        g["moves"][2]["eval"] = 1000000
        rows = labels.candidates(g)
        self.assertTrue(all(r["center_ply"] >= 3 for r in rows))
        self.assertFalse(any(r["selection"] == "decisive_ending" for r in rows))
        g["moves"] = g["moves"][3:4]
        self.assertEqual(len(labels.candidates(g)), 1)
        g["white"]["engine"] = "old"
        self.assertEqual(labels.candidates(g), [])

    def test_mate_windows_offsets_claim_direction_and_retraction(self):
        g = self.game()
        for m in g["moves"]:
            m["eval"] = 10
        g["moves"][40]["eval"] = 999990
        g["moves"][41]["eval"] = 999989
        g["moves"][42]["eval"] = 999988
        windows = labels.mate_windows(g)
        self.assertEqual(set(windows), {24, 32, 36, 38, 40, 25, 33, 37, 39, 41})
        self.assertEqual(windows[40][0]["claim"], "winning")
        self.assertEqual(windows[41][0]["claim"], "losing")
        self.assertEqual(windows[40][0]["retraction_ply"], 45)
        self.assertEqual(windows[41][0]["retraction_ply"], 44)
        rows = labels.candidates(g)
        self.assertEqual(len(rows), len({r["center_ply"] for r in rows}))
        self.assertLessEqual(len(rows), 27)
        self.assertEqual({r["center_ply"] - 1 for r in rows if "mate_recognitions" in r}, set(windows))
        self.assertFalse(any(r["selection"] == "decisive_ending" for r in rows))

    def test_first_claim_only_draws_boundaries_and_persistence(self):
        g = self.game()
        g["result"] = "Draw"
        g["moves"] = [dict(color="Black", eval=-999999), dict(color="White", eval=0), dict(color="Black", eval=-999998)]
        windows = labels.mate_windows(g)
        self.assertEqual(set(windows), {0})
        self.assertEqual(windows[0][0]["claim"], "losing")
        self.assertEqual(windows[0][0]["persistence"], "persisted")
        g["moves"][2]["eval"] = None
        self.assertEqual(labels.mate_windows(g)[0][0]["persistence"], "unknown")
        g["moves"][2]["eval"] = 999998
        self.assertEqual(labels.mate_windows(g)[0][0]["persistence"], "retracted")

    def test_equal_mass_spreads_constant_disagreement(self):
        from unittest.mock import patch

        g = self.game()
        with patch.object(labels, "disagreement", return_value=0.3):
            selected = labels.ordinary_positions(g, list(range(200)), {50, 120}, "test")
        self.assertEqual(len(selected), 12)
        anchors = {50, 120}
        for i, metadata in selected.items():
            self.assertTrue(all(abs(i - j) >= 10 for j in anchors))
            anchors.add(i)
            self.assertEqual(metadata["sampling_weight"], 2.5)
            self.assertLess(abs(metadata["selected_mass_fraction"] - metadata["target_mass_fraction"]), 0.025)

    def test_broad_phase_is_not_crowded_out_by_slightly_higher_peak(self):
        from unittest.mock import patch

        g = self.game()
        g["moves"] = [dict(color="Black" if i % 2 == 0 else "White", eval=0) for i in range(300)]
        with patch.object(labels, "disagreement", side_effect=lambda _, i: 0.45 if 125 <= i < 175 else 0.4):
            selected = labels.ordinary_positions(g, list(range(300)), [], "test")
        self.assertEqual(len(selected), 12)
        self.assertEqual(sum(125 <= i < 175 for i in selected), 2)
        self.assertTrue(all(b - a >= 10 for a, b in zip(sorted(selected), sorted(selected)[1:], strict=False)))

    def test_adjacent_score_average_and_probability_scale(self):
        g = self.game()
        g["moves"][:3] = [dict(color="Black", eval=0), dict(color="White", eval=1000), dict(color="Black", eval=2000)]
        self.assertEqual(labels.disagreement(g, 1), 0)
        g["moves"][0]["eval"] = True
        self.assertIsNone(labels.disagreement(g, 1))

    def test_mate_exclusion_and_unknown_disagreement(self):
        g = self.game()
        g["moves"][80]["eval"] = 999999
        valid = list(range(200))
        excluded = labels.mate_exclusions(g)
        selected = labels.ordinary_positions(g, valid, {20}, "test")
        self.assertFalse(set(selected) & excluded)
        self.assertEqual(selected, labels.ordinary_positions(g, valid, {20}, "test"))
        self.assertIsNone(labels.disagreement(g, 80))
        g["moves"] = [dict(color="Black", eval=0)]
        selected = labels.ordinary_positions(g, [0], [], "test")
        self.assertEqual(selected[0]["ordinary_role"], "weighted_mass")
        self.assertIsNone(selected[0]["disagreement"])


class LabelSearchTests(AnalyzerFixture):
    def test_mate_backfill_preserves_normalized_labels_without_moment_copies(self):
        (self.run / "analysis/moments").rmdir()
        game = SamplingTests().game()
        game["game_id"] = "normalized-mate-fixture"
        for move in game["moves"]:
            move["eval"] = 0
        game["moves"][40]["eval"] = 999999
        path = self.run / "game.json"
        a.atomic(path, game)
        game_hash = a.digest(path.read_bytes())
        search = dict(
            key="saved-search",
            score=23,
            completed_depth=3,
            model_sha256="frozen-model",
            game_hash=game_hash,
            ply=39,
            original_move=game["moves"][38],
        )
        previous = dict(search, key="previous-search", model_sha256="previous-model", score=17)
        existing = dict(
            id="old",
            center_ply=39,
            game_hash=game_hash,
            searches=[search],
            previous_teacher_searches=[previous],
        )
        packed = label_storage.encode(self.db, existing)
        self.db.execute("INSERT INTO moments VALUES('old',0,'completed',?,NULL)", (packed,))
        self.db.execute("CREATE TABLE dual_paired(id TEXT PRIMARY KEY)")
        self.db.execute("INSERT INTO dual_paired VALUES('old')")
        self.db.execute("INSERT INTO files VALUES(?,?)", (str(path), game_hash))
        self.db.commit()
        a.atomic(self.run / "state.json", dict(slots=[dict(id=1, status="done", game_path=str(path))]))

        for _ in range(2):
            a.scan(self.db, self.run, labels=True, label_candidates=lambda _: [])

        self.assertEqual(self.db.execute("SELECT count(*) FROM moments").fetchone()[0], 5)
        row = self.db.execute("SELECT * FROM moments WHERE id='old'").fetchone()
        self.assertEqual(row["status"], "completed")
        self.assertIsNone(row["error"])
        payload = a.json.loads(row["payload"])
        self.assertEqual(payload["searches"], [{"search_ref": "saved-search"}])
        self.assertEqual(payload["previous_teacher_searches"], [{"search_ref": "previous-search"}])
        decoded = label_storage.decode(self.db, payload)
        self.assertEqual(decoded["searches"], [search])
        self.assertEqual(decoded["previous_teacher_searches"], [previous])
        self.assertEqual(decoded["mate_recognitions"][0]["offset_plies"], -2)
        self.assertEqual(self.db.execute("SELECT id FROM dual_paired").fetchall()[0]["id"], "old")
        self.assertEqual(self.db.execute("SELECT count(*) FROM searches").fetchone()[0], 2)
        self.assertEqual(
            self.db.execute(
                "SELECT count(*) FROM mate_scanned WHERE policy=? AND path=?", (labels.MATE_POLICY, str(path))
            ).fetchone()[0],
            1,
        )
        self.assertFalse((self.run / "analysis/moments").exists())

    def test_mate_backfill_reuses_completed_position_and_is_idempotent(self):
        g = SamplingTests().game()
        g["game_id"] = "mate-fixture"
        for m in g["moves"]:
            m["eval"] = 0
        g["moves"][40]["eval"] = 999999
        path = self.run / "game.json"
        a.atomic(path, g)
        game_hash = a.digest(path.read_bytes())
        existing = dict(id="old", center_ply=39, game_hash=game_hash, searches=[{"score": 23}])
        self.db.execute("INSERT INTO moments VALUES('old',0,'completed',?,NULL)", (a.json.dumps(existing),))
        self.db.execute("INSERT INTO files VALUES(?,?)", (str(path), game_hash))
        self.db.commit()
        a.atomic(self.run / "state.json", dict(slots=[dict(id=1, status="done", game_path=str(path))]))
        # The special winner sampler can skip the ordinary path; mate coverage still applies.
        a.scan(self.db, self.run, labels=True, label_candidates=lambda _: [])
        a.scan(self.db, self.run, labels=True, label_candidates=lambda _: [])
        self.assertEqual(self.db.execute("SELECT count(*) FROM moments").fetchone()[0], 5)
        row = self.db.execute("SELECT * FROM moments WHERE id='old'").fetchone()
        payload = a.json.loads(row["payload"])
        self.assertEqual(row["status"], "completed")
        self.assertEqual(payload["searches"], [{"score": 23}])
        self.assertEqual(payload["mate_recognitions"][0]["offset_plies"], -2)

    def test_teacher_fixed_budget_iterations_and_independent_catalogue(self):
        g = self.game()
        model = self.run / "teacher.json"
        model.write_text("{}")
        path = self.run / "game.json"
        a.atomic(path, g)
        helper = self.run / "helper"
        helper.write_text(
            "#!/usr/bin/env python3\nimport json,sys\n"
            "for d in (1,2): print(json.dumps(dict(completed_depth=d,score=d,args=sys.argv[1:])),flush=True)\n"
        )
        helper.chmod(0o755)
        teacher = {"name": "ab", "model": str(model)}
        cfg = dict(
            run=str(self.run),
            label_teacher=teacher,
            models={str(model): {"sha256": a.digest(model.read_bytes()), "snapshot": str(model)}},
            analyzer_bin=str(helper),
            analyzer_sha256="fixture",
        )
        result = a.search_position(cfg, {"game": str(path), "game_hash": a.digest(path.read_bytes())}, 2, self.db)
        self.assertEqual(result["agent"], teacher)
        self.assertEqual(result["original_agent"], g["white"])
        self.assertEqual(result["args"][-2:], ["64", "10000"])
        self.assertEqual([r["completed_depth"] for r in result["iterations"]], [1, 2])
        db = a.connect(self.run, a.catalogue(cfg))
        self.assertEqual(db.execute("select count(*) from searches").fetchone()[0], 0)
        a.atomic(self.run / "state.json", {"slots": [{"id": 1, "status": "done", "game_path": str(path)}]})
        a.scan(db, self.run, labels=True)
        a.scan(db, self.run, labels=True)
        self.assertEqual(db.execute("select count(*) from files").fetchone()[0], 1)
        db.close()

    def test_teacher_budget_override_has_distinct_cache_key(self):
        g = self.game()
        model = self.run / "teacher.json"
        model.write_text("{}")
        path = self.run / "game.json"
        a.atomic(path, g)
        helper = self.run / "helper"
        helper.write_text(
            "#!/usr/bin/env python3\nimport json,sys\n"
            "print(json.dumps(dict(completed_depth=2,score=1,args=sys.argv[1:])),flush=True)\n"
        )
        helper.chmod(0o755)
        cfg = dict(
            run=str(self.run),
            label_teacher={"name": "ab", "model": str(model)},
            models={str(model): {"sha256": a.digest(model.read_bytes()), "snapshot": str(model)}},
            analyzer_bin=str(helper),
            analyzer_sha256="fixture",
        )
        moment = {"game": str(path), "game_hash": a.digest(path.read_bytes())}
        short = a.search_position(cfg, moment, 2, self.db)
        longer = a.search_position(dict(cfg, label_budget_ms=30000), moment, 2, self.db)
        self.assertEqual(short["budget_ms"], 10000)
        self.assertEqual(longer["budget_ms"], 30000)
        self.assertNotEqual(short["key"], longer["key"])
