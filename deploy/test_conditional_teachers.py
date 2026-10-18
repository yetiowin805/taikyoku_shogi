import json
from unittest.mock import patch

import conditional_teachers as c
import dual_label_sidecar as dual
from test_tourney_analysis import AnalyzerFixture, a


def game(black, white, result="BlackWins", side="Black"):
    other = "White" if side == "Black" else "Black"
    scores = {"Black": black, "White": white}
    return dict(
        black=dict(name="ab", model="black"),
        white=dict(name="ab", model="white"),
        result=result,
        moves=[
            dict(color=other, eval=scores[other] - 10),
            dict(color=side, eval=scores[side]),
            dict(color=other, eval=scores[other] + 10),
        ],
    )


class ConditionalTests(AnalyzerFixture):
    def test_rules_both_colors_and_threshold(self):
        cases = [
            (100, -100, "BlackWins", "black", "mutual_optimism"),
            (100, -100, "WhiteWins", "white", "mutual_optimism"),
            (-100, 100, "BlackWins", None, "no_rule_matched"),
            (120, 100, "BlackWins", "black", "winner_advantage_20_percent"),
            (119, 100, "BlackWins", None, "no_rule_matched"),
            (-100, -120, "WhiteWins", "white", "winner_advantage_20_percent"),
            (120, 100, "WhiteWins", None, "no_rule_matched"),
            (100, 30, "Draw", "white", "draw_more_conservative"),
            (-30, -100, "Draw", "black", "draw_more_conservative"),
            (100, -100, "Draw", None, "no_rule_matched"),
            (0, 100, "Draw", None, "no_rule_matched"),
            (100, 100, "Draw", "black", "draw_more_conservative"),
        ]
        for black, white, result, expected, reason in cases:
            for side in ("Black", "White"):
                with self.subTest(black=black, white=white, result=result, side=side):
                    agent, info = c.select(game(black, white, result, side), 2)
                    self.assertEqual(agent["model"] if agent else None, expected)
                    self.assertEqual(info["reason"], reason)
                    self.assertEqual((info["black_estimate"], info["white_estimate"]), (black, white))

    def test_missing_boundary_and_malformed_neighbors(self):
        g = game(120, 100)
        for ply in (1, 3):
            self.assertIsNone(c.select(g, ply)[0])
        for bad in (None, True, "100"):
            g["moves"][0]["eval"] = bad
            self.assertIsNone(c.select(g, 2)[0])

    def test_primary_persisted_on_failure_and_archive_excluded(self):
        path = self.run / "conditional-game.json"
        path.write_text(json.dumps(game(100, -100)))
        primary_model = str((self.run / "primary").resolve())
        secondary_model = str(a.absolute("black").resolve())
        config = dict(
            teacher_policy=c.POLICY,
            budget_ms=30000,
            old=dict(models={secondary_model: dict(sha256="secondary")}),
            new_teacher_id="NNUE_W512_v4",
            new_model=primary_model,
            new_binding=dict(sha256="primary"),
            new_sha256="primary",
        )
        moment = dict(
            id="m",
            center_ply=2,
            game=str(path),
            game_hash=a.digest(path.read_bytes()),
            searches=[dict(model_sha256="obsolete", key="old")],
        )
        self.db.execute("INSERT INTO moments VALUES('m',0,'pending',?,NULL)", (json.dumps(moment),))
        self.db.commit()
        primary = dict(model_sha256="primary", key="p")
        with patch.object(dual.a, "search_position", side_effect=[primary, InterruptedError()]):
            with self.assertRaises(InterruptedError):
                dual.paired_searches(config, moment, self.db)
        saved = json.loads(self.db.execute("SELECT payload FROM moments").fetchone()[0])
        self.assertEqual(saved["searches"], [primary])
        self.assertEqual(saved["previous_teacher_searches"][0]["model_sha256"], "obsolete")
        # Retry goes through the content-addressed search cache, then finishes the secondary.
        secondary = dict(model_sha256="secondary", key="s")
        with patch.object(dual.a, "search_position", side_effect=[primary, secondary]):
            result = dual.paired_searches(config, saved, self.db)
        self.assertEqual(result["searches"], [primary, secondary])

    def test_requeue_is_once_and_keeps_search_cache(self):
        self.db.executescript("CREATE TABLE dual_paired(id TEXT PRIMARY KEY); CREATE TABLE dual_failures(id TEXT);")
        self.db.execute("INSERT INTO moments VALUES('m',0,'completed','{}',NULL)")
        self.db.execute("INSERT INTO dual_paired VALUES('m')")
        self.db.execute("INSERT INTO searches VALUES('cached','{}')")
        self.db.commit()
        c.requeue(self.db, dict(teacher_policy=c.POLICY))
        self.assertEqual(self.db.execute("SELECT count(*) FROM dual_paired").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT count(*) FROM searches").fetchone()[0], 1)
        self.db.execute("INSERT INTO dual_paired VALUES('m')")
        self.db.commit()
        c.requeue(self.db, dict(teacher_policy=c.POLICY))
        self.assertEqual(self.db.execute("SELECT count(*) FROM dual_paired").fetchone()[0], 1)

    def test_no_secondary_and_duplicate_primary(self):
        for scores in ((-100, 100), (100, -100)):
            with self.subTest(scores=scores):
                primary_model = str((self.run / "primary").resolve())
                g = game(*scores)
                g["black"]["model"] = primary_model
                path = self.run / "game.json"
                path.write_text(json.dumps(g))
                config = dict(
                    teacher_policy=c.POLICY,
                    budget_ms=30000,
                    old=dict(models={}),
                    new_teacher_id="NNUE_W512_v4",
                    new_model=primary_model,
                    new_binding=dict(sha256="primary"),
                    new_sha256="primary",
                )
                moment = dict(id="m", center_ply=2, game=str(path), game_hash=a.digest(path.read_bytes()))
                self.db.execute("INSERT OR REPLACE INTO moments VALUES('m',0,'pending',?,NULL)", (json.dumps(moment),))
                self.db.commit()
                with patch.object(
                    dual.a,
                    "search_position",
                    return_value=dict(
                        model_sha256="primary",
                        budget_ms=30000,
                        game_hash=moment["game_hash"],
                        ply=2,
                        score=17,
                        completed_depth=4,
                    ),
                ) as search:
                    result = dual.paired_searches(config, moment, self.db)
                self.assertEqual(search.call_count, 1)
                self.assertEqual(len(result["searches"]), 1)
                with patch.object(dual.a, "search_position") as reused:
                    again = dual.paired_searches(config, result, self.db)
                reused.assert_not_called()
                self.assertEqual(again["searches"][0]["score"], 17)
