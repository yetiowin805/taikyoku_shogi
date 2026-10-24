import json
from unittest.mock import patch

import dual_label_sidecar as dual
import label_storage
import teacher_refresh as refresh
from test_tourney_analysis import AnalyzerFixture


class TeacherRefreshTests(AnalyzerFixture):
    def setup_catalogue(self):
        self.db.executescript(
            "CREATE TABLE dual_paired(id TEXT PRIMARY KEY); CREATE TABLE dual_failures(id TEXT PRIMARY KEY,error TEXT,updated REAL);"
        )

    def add(self, moment, status="completed", paired=True):
        self.db.execute("INSERT INTO moments VALUES(?,0,?,?,NULL)", (moment["id"], status, json.dumps(moment)))
        if paired:
            self.db.execute("INSERT INTO dual_paired VALUES(?)", (moment["id"],))
        self.db.commit()

    def test_switch_only_publishes_validated_snapshot_without_opening_catalogue(self):
        a = dual.a
        model = self.run / "teacher.json"
        model.write_text('{"name":"new teacher"}')
        path = self.run / "analysis/dual-label-config.json"
        a.atomic(path, dict(old={}, new_sha256="old", analyzer_bin="fake-helper"))
        original = path.read_bytes()
        with (
            patch.object(a, "validate_source"),
            patch.object(a.subprocess, "run"),
            patch.object(a, "connect", side_effect=AssertionError("switch must not open the catalogue")),
            patch.object(a, "snapshot_model", return_value={"snapshot": "pinned"}) as snapshot,
        ):
            self.assertEqual(refresh.replace(a, self.run, model, check_only=True)["backfill_positions"], 0)
            snapshot.assert_not_called()
            self.assertEqual(path.read_bytes(), original)
            snapshot.side_effect = OSError("disk full")
            with self.assertRaisesRegex(OSError, "disk full"):
                refresh.replace(a, self.run, model)
            self.assertEqual(path.read_bytes(), original)
            snapshot.side_effect = None
            report = refresh.replace(a, self.run, model)
            self.assertTrue(report["changed"])
            self.assertEqual(report["backfill_positions"], 0)
            self.assertEqual(a.read(path)["new_teacher_id"], "new teacher")
            self.assertNotIn("teacher_refresh", a.read(path))
            snapshot.reset_mock()
            self.assertFalse(refresh.replace(a, self.run, model)["changed"])
            snapshot.assert_not_called()

    def migration_fixture(self):
        self.setup_catalogue()
        self.config = dict(teacher_refresh=dict(id="latest", ids=["refresh", "completed"]))
        self.path = self.run / "analysis/dual-label-config.json"
        dual.a.atomic(self.path, self.config)
        label = dict(score=42, model_sha256="old", game_hash="game", ply=2, completed_depth=3)
        for key in ("refresh", "failed", "archived", "completed", "new", "invalid", "other-failure"):
            m = dict(id=key, game_hash="game", center_ply=2, searches=[label])
            if key not in ("new", "other-failure"):
                m["teacher_refresh"] = "older" if key == "archived" else "latest"
            if key == "archived":
                m.update(searches=[], previous_teacher_searches=[label])
            if key == "invalid":
                m["searches"] = [dict(label, game_hash="wrong-game")]
            if key == "new":
                m["searches"] = []
            done = key == "completed"
            self.add(m, status="completed" if done else "pending", paired=done)
        self.db.executemany(
            "INSERT INTO dual_failures VALUES(?,?,0)", [("failed", "old error"), ("other-failure", "keep")]
        )
        self.db.execute("INSERT INTO searches VALUES('cached','unchanged')")
        self.db.commit()

    def test_retirement_preserves_labels_new_work_and_completed_refreshes(self):
        self.migration_fixture()
        payloads = [tuple(r) for r in self.db.execute("SELECT id,payload FROM moments ORDER BY id")]
        original_config = self.path.read_bytes()
        report = refresh.retire_backfill(dual.a, self.run, self.db, self.config)
        self.assertEqual(report["cancelled"], 3)
        self.assertEqual(report["without_saved_label"], 1)
        self.assertEqual(
            {r[0] for r in self.db.execute("SELECT id FROM dual_paired")},
            {"refresh", "failed", "archived", "completed"},
        )
        self.assertEqual([r[0] for r in self.db.execute("SELECT id FROM dual_failures")], ["other-failure"])
        self.assertEqual([tuple(r) for r in self.db.execute("SELECT id,payload FROM moments ORDER BY id")], payloads)
        self.assertEqual(self.db.execute("SELECT payload FROM searches").fetchone()[0], "unchanged")
        self.assertEqual(dict(self.db.execute("SELECT id,status FROM moments"))["new"], "pending")
        backup = self.run / "analysis/teacher-backups"
        saved = next(p for p in backup.iterdir() if p.name.isdigit())
        self.assertTrue((saved / "training-labels.sqlite.gz").is_file())
        self.assertEqual((saved / "dual-label-config.json").read_bytes(), original_config)
        self.assertNotIn("teacher_refresh", self.config)
        with patch.object(refresh.artifact_storage, "teacher_backup", side_effect=AssertionError("repeat backup")):
            self.assertEqual(refresh.retire_backfill(dual.a, self.run, self.db, self.config), report)

    def test_migration_backup_transaction_and_publication_failures_are_recoverable(self):
        self.migration_fixture()
        original = self.path.read_bytes()
        with patch.object(refresh.artifact_storage, "teacher_backup", side_effect=OSError("disk full")):
            with self.assertRaisesRegex(OSError, "disk full"):
                refresh.retire_backfill(dual.a, self.run, self.db, self.config)
        self.assertEqual(self.path.read_bytes(), original)
        self.db.execute(
            "CREATE TRIGGER fail_insert BEFORE INSERT ON dual_paired BEGIN SELECT RAISE(ABORT,'rollback'); END"
        )
        with patch.object(refresh.artifact_storage, "teacher_backup", return_value=None):
            with self.assertRaisesRegex(Exception, "rollback"):
                refresh.retire_backfill(dual.a, self.run, self.db, self.config)
            self.assertEqual(self.db.execute("SELECT count(*) FROM dual_paired").fetchone()[0], 1)
            self.assertEqual(self.db.execute("SELECT count(*) FROM analysis_migrations").fetchone()[0], 0)
            self.assertEqual(dict(self.db.execute("SELECT id,status FROM moments"))["refresh"], "pending")
            self.db.execute("DROP TRIGGER fail_insert")
            with patch.object(dual.a, "atomic", side_effect=OSError("publication failed")):
                with self.assertRaisesRegex(OSError, "publication failed"):
                    refresh.retire_backfill(dual.a, self.run, self.db, self.config)
        self.assertEqual(self.path.read_bytes(), original)
        with patch.object(refresh.artifact_storage, "teacher_backup", side_effect=AssertionError("repeat backup")):
            report = refresh.retire_backfill(dual.a, self.run, self.db, self.config)
        self.assertEqual(report["cancelled"], 3)
        self.assertEqual(dual.a.read(self.path)["backfill_retirement"], report)

    def test_second_search_interruption_archives_old_default_immediately(self):
        self.setup_catalogue()
        first = dict(model_sha256="first", budget_ms=30000, key="a")
        previous = dict(model_sha256="2048", budget_ms=30000, key="b")
        moment = dict(id="m", center_ply=2, searches=[first, previous])
        self.add(moment)
        config = dict(
            old=dict(label_teacher=dict(name="ab", model="first"), models={"first": {"sha256": "first"}}),
            budget_ms=30000,
            new_model="seeds",
            new_sha256="seeds",
            new_binding={"sha256": "seeds"},
            new_teacher_id="SEEDS2",
        )
        with patch.object(dual.a, "search_position", side_effect=InterruptedError):
            with self.assertRaises(InterruptedError):
                dual.paired_searches(config, moment, self.db)
        saved = label_storage.decode(self.db, self.db.execute("SELECT payload FROM moments").fetchone()[0])
        self.assertEqual(saved["previous_teacher_searches"], [previous])
        new = dict(model_sha256="seeds", budget_ms=30000, key="c")
        with patch.object(dual.a, "search_position", return_value=new) as search:
            result = dual.paired_searches(config, saved, self.db)
        self.assertEqual(search.call_count, 1)
        self.assertEqual(search.call_args.args[0]["label_teacher_id"], "SEEDS2")
        self.assertEqual(result["searches"], [first, new])
        self.assertEqual(result["previous_teacher_searches"], [previous])

    def test_same_winner_and_default_search_only_once(self):
        self.setup_catalogue()
        moment = dict(id="m", center_ply=2, searches=[])
        self.add(moment)
        config = dict(
            old=dict(label_teacher=dict(name="ab", model="seeds"), models={"seeds": {"sha256": "seeds"}}),
            budget_ms=30000,
            new_model="seeds",
            new_sha256="seeds",
            new_binding={"sha256": "seeds"},
        )
        result = dict(model_sha256="seeds", budget_ms=30000, key="a")
        with patch.object(dual.a, "search_position", return_value=result) as search:
            self.assertEqual(dual.paired_searches(config, moment, self.db)["searches"], [result])
        self.assertEqual(search.call_count, 1)
