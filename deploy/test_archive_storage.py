import json
import multiprocessing
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

import archive_storage as storage


def hold_pin(config, path, ready, release):
    with storage.Archive(config).pin([path]):
        ready.set()
        release.wait(20)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.config = dict(
            root=str(self.root / "control"),
            remote=str(self.root / "remote"),
            local_remote=True,
            min_free_bytes=0,
            pack_bytes=1024,
        )
        self.archive = storage.Archive(self.config)
        self.source = self.root / "features.bin"
        self.bytes = bytes(range(256)) * 12
        self.source.write_bytes(self.bytes)

    def test_full_readback_restore_different_target_and_remote_index_recovery(self):
        self.archive.archive([self.source], "features")
        self.assertTrue(self.archive.verify_restore(self.source))
        self.assertEqual(self.archive.evict([self.source]), len(self.bytes))
        self.assertFalse(self.source.exists())
        recovered = storage.Archive(dict(self.config, root=str(self.root / "recovered")))
        recovered.recover_index()
        with recovered.pin([self.source]):
            self.assertEqual(self.source.read_bytes(), self.bytes)
        self.assertEqual(self.archive.archive([self.source], "features"), 0)
        self.source.write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "different existing|changed"):
            self.archive.hydrate(self.source)

    def test_corrupt_readback_keeps_original_and_retry_is_safe(self):
        real = self.archive.download

        def corrupt(source, target, size):
            real(source, target, size)
            Path(target).write_bytes(b"corrupt")

        with patch.object(self.archive, "download", side_effect=corrupt):
            with self.assertRaisesRegex(ValueError, "hash mismatch"):
                self.archive.archive([self.source], "features")
        self.assertEqual(self.source.read_bytes(), self.bytes)
        self.archive.archive([self.source], "features")
        self.assertEqual(self.archive.evict([self.source]), len(self.bytes))
        with self.archive.locked() as db:
            pack = db.execute("SELECT hash FROM packs").fetchone()[0]
        (self.root / "remote/objects" / f"{pack}.tar.zst").write_bytes(b"corrupt")
        with self.assertRaises(ValueError):
            self.archive.hydrate(self.source)
        self.assertFalse(self.source.exists())

    def test_other_process_pin_prevents_eviction_until_release(self):
        self.archive.archive([self.source], "features")
        ctx = multiprocessing.get_context("spawn")
        ready, release = ctx.Event(), ctx.Event()
        process = ctx.Process(target=hold_pin, args=(self.config, str(self.source), ready, release))
        process.start()
        try:
            self.assertTrue(ready.wait(10))
            self.assertEqual(self.archive.evict([self.source]), 0)
        finally:
            release.set()
            process.join(10)
            if process.is_alive():
                process.kill()
                process.join()
        self.assertEqual(process.exitcode, 0)
        self.assertEqual(self.archive.evict([self.source]), len(self.bytes))

    def test_peak_reservations_are_combined_released_and_dead_pids_reaped(self):
        usage = type("Usage", (), {"free": 1000})()
        with patch("archive_storage.shutil.disk_usage", return_value=usage):
            with self.archive.pin([], {self.root: 700}):
                with self.assertRaisesRegex(ValueError, "Insufficient peak"):
                    self.archive.pin([], {self.root: 400})
            with self.archive.pin([], {self.root: 900}):
                pass
            with self.archive.locked() as db:
                db.execute(
                    "INSERT INTO reservations VALUES(?,?,?,?,?)",
                    ("dead", 999999, "missing", os.stat(self.root).st_dev, 999),
                )
            with self.archive.pin([], {self.root: 900}):
                pass

    def test_no_automatic_eviction_of_games_models_or_changed_features(self):
        self.archive.archive([self.source], "models")
        self.assertEqual(self.archive.evict(cache_bytes=0), 0)
        feature = self.root / "other.bin"
        feature.write_bytes(self.bytes)
        self.archive.archive([feature], "features")
        feature.write_bytes(b"different")
        with self.assertRaisesRegex(ValueError, "changed eviction"):
            self.archive.evict([feature])

    def test_failed_index_publication_does_not_allow_local_removal(self):
        with patch.object(self.archive, "publish_index", side_effect=RuntimeError("index interrupted")):
            with self.assertRaises(RuntimeError):
                self.archive.archive([self.source], "features")
        self.assertEqual(self.archive.evict([self.source]), 0)
        self.assertTrue(self.source.exists())

    def test_import_verified_legacy_archive_and_restore_exact_original(self):
        import subprocess

        compressed = self.root / "legacy.zst"
        subprocess.run(["zstd", "-q", str(self.source), "-o", str(compressed)], check=True)
        h = storage.digest(self.source)
        manifest = self.root / "legacy.json"
        manifest.write_text(
            json.dumps(
                dict(
                    state="completed",
                    files=[
                        dict(
                            path=str(self.source),
                            bytes=len(self.bytes),
                            mode=0o640,
                            sha256=h,
                            archive_sha256=storage.digest(compressed),
                            archive_bytes=compressed.stat().st_size,
                            remote=str(compressed),
                            encoding="zstd",
                            local_evicted=True,
                            full_download_and_restore_verified_at=1,
                        )
                    ],
                )
            )
        )
        self.source.unlink()
        imported = storage.Archive(dict(self.config, legacy_manifest=str(manifest)))
        imported.hydrate(self.source)
        self.assertEqual(self.source.read_bytes(), self.bytes)
        self.assertEqual(self.source.stat().st_mode & 0o777, 0o640)

    def test_completed_continuation_can_replace_its_resume_generation(self):
        self.archive.archive([self.source], "closed-training")
        self.archive.evict([self.source])
        with self.archive.pin([self.source], mutable_paths=[self.source]):
            self.assertEqual(self.source.read_bytes(), self.bytes)
            self.source.write_bytes(b"next completed checkpoint")
            self.assertEqual(self.archive.evict([self.source]), 0)
        self.archive.archive([self.source], "closed-training")
        self.archive.evict([self.source])
        self.archive.hydrate(self.source)
        self.assertEqual(self.source.read_bytes(), b"next completed checkpoint")
        self.assertEqual(len(list((self.root / "remote/objects").iterdir())), 2)

    def test_identical_alias_reuses_verified_pack_and_preserves_its_mode(self):
        self.archive.archive([self.source], "features")
        alias = self.root / "same-bytes.bin"
        alias.write_bytes(self.bytes)
        alias.chmod(0o400)
        self.assertEqual(self.archive.archive([alias], "features"), 1)
        self.assertEqual(len(list((self.root / "remote/objects").iterdir())), 1)
        self.archive.evict([alias])
        self.archive.hydrate(alias)
        self.assertEqual(alias.read_bytes(), self.bytes)
        self.assertEqual(alias.stat().st_mode & 0o777, 0o400)

    def test_new_aliases_in_one_call_store_the_payload_once(self):
        import io
        import subprocess
        import tarfile

        alias = self.root / "same-batch.bin"
        alias.write_bytes(self.bytes)
        self.archive.archive([self.source, alias], "features")
        objects = list((self.root / "remote/objects").iterdir())
        self.assertEqual(len(objects), 1)
        data = subprocess.check_output(["zstd", "-q", "-d", "-c", str(objects[0])])
        with tarfile.open(fileobj=io.BytesIO(data)) as pack:
            self.assertEqual(len(pack.getmembers()), 1)
        self.archive.evict([self.source, alias])
        self.archive.hydrate(alias)
        self.assertEqual(alias.read_bytes(), self.bytes)

    def test_optional_writer_stops_at_soft_target_above_hard_reserve(self):
        self.archive.config["filesystems"] = [dict(path=str(self.root), min_free_bytes=100, soft_free_bytes=500)]
        with self.archive.pin([self.source]) as lease:
            with patch("archive_storage.shutil.disk_usage", return_value=type("Usage", (), {"free": 499})()):
                with self.assertRaisesRegex(RuntimeError, "soft target"):
                    lease.check_headroom()
        self.assertTrue(self.source.exists())

    def test_declared_dataset_hash_must_match_a_verified_recovery_entry(self):
        with self.assertRaisesRegex(ValueError, "dataset dependency"):
            self.archive.require(self.source, storage.digest(self.source))
        self.archive.archive([self.source], "features")
        self.archive.require(self.source, storage.digest(self.source))
        with self.assertRaisesRegex(ValueError, "dataset dependency"):
            self.archive.require(self.source, "0" * 64)
