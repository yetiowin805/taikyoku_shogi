import contextlib
import gzip
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from unittest.mock import patch

import artifact_storage as storage


class ArtifactStorageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def database(self, path):
        with contextlib.closing(sqlite3.connect(path)) as db:
            db.execute('CREATE TABLE labels(id INTEGER PRIMARY KEY, value TEXT)')
            db.execute('INSERT INTO labels VALUES(1,?)', ('useful label'*10000,))
            db.commit()

    def test_shared_snapshots_survive_original_mutation_and_removal(self):
        original = self.root/'original.bin'; original.write_bytes(b'network')
        sha = storage.digest(original)
        store = self.root/'store'
        with patch.object(storage, 'settings', return_value=dict(model_store=str(store))):
            for name in ('first.nnue', 'second.nnue'):
                storage.snapshot_blob(original, self.root/name, sha)
        first, second = self.root/'first.nnue', self.root/'second.nnue'
        self.assertTrue(first.is_symlink())
        self.assertEqual(first.resolve(), second.resolve())
        self.assertNotEqual(first.stat().st_ino, original.stat().st_ino)
        original.write_bytes(b'mutated'); original.unlink()
        self.assertEqual(first.read_bytes(), b'network')
        self.assertEqual(len(list(store.glob('*.nnue'))), 1)

    def test_existing_snapshot_migration_preserves_bytes_and_rejects_corruption(self):
        target = self.root/'old.nnue'; target.write_bytes(b'network')
        sha = storage.digest(target)
        store = self.root/'store'
        with patch.object(storage, 'settings', return_value=dict(model_store=str(store))):
            storage.snapshot_blob(target, target, sha)
            self.assertTrue(target.is_symlink())
            self.assertEqual(storage.digest(target), sha)
            bad = self.root/'bad.nnue'; bad.write_bytes(b'wrong')
            with self.assertRaisesRegex(ValueError, 'corrupt existing'):
                storage.snapshot_blob(target, bad, sha)
            self.assertEqual(bad.read_bytes(), b'wrong')

    def test_compression_round_trip_and_disk_failure_preserve_original(self):
        path = self.root/'labels.sqlite'; self.database(path)
        original = path.read_bytes()
        with patch.object(storage.gzip, 'GzipFile', side_effect=OSError('disk full')):
            with self.assertRaisesRegex(OSError, 'disk full'):
                storage.compress_database(path)
        self.assertEqual(path.read_bytes(), original)
        result = storage.compress_database(path)
        self.assertLess(result['compressed_bytes'], result['original_bytes'])
        self.assertFalse(path.exists())
        self.assertEqual(gzip.decompress((self.root/'labels.sqlite.gz').read_bytes()), original)

    def test_hot_journal_is_preserved(self):
        path = self.root/'labels.sqlite'; self.database(path)
        Path(str(path)+'-journal').write_bytes(b'unfinished transaction')
        with self.assertRaisesRegex(ValueError, 'journal'):
            storage.compress_database(path)
        self.assertTrue(path.exists())

    def test_completed_backups_are_bounded_and_legacy_is_retained(self):
        control = self.root/'analysis'; control.mkdir()
        config = control/'dual-label-config.json'; config.write_text('{"teacher":"old"}')
        path = self.root/'live.sqlite'; self.database(path)
        with contextlib.closing(sqlite3.connect(path)) as db:
            backups = [storage.teacher_backup(db, config) for _ in range(5)]
        legacy = control/'teacher-backups'/'100'; legacy.mkdir()
        (legacy/'training-labels.sqlite').write_bytes(b'legacy evidence')
        incomplete = control/'teacher-backups'/'.incomplete-test'; incomplete.mkdir()
        storage.prune_teacher_backups(control/'teacher-backups')
        self.assertEqual([p.exists() for p in backups], [False, False, True, True, True])
        self.assertTrue(legacy.exists()); self.assertTrue(incomplete.exists())
        restored = self.root/'restored.sqlite'
        restored.write_bytes(gzip.decompress((backups[-1]/'training-labels.sqlite.gz').read_bytes()))
        with contextlib.closing(sqlite3.connect(restored)) as db:
            self.assertEqual(db.execute('SELECT value FROM labels').fetchone()[0], 'useful label'*10000)
        self.assertEqual(json.loads((backups[-1]/'dual-label-config.json').read_text()), {'teacher':'old'})


if __name__ == '__main__':
    unittest.main()
