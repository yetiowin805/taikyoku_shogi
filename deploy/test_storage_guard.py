import unittest
from unittest.mock import patch
from types import SimpleNamespace
import storage_guard as guard


class StorageGuardTests(unittest.TestCase):
    def test_missing_mount_is_fatal_even_if_root_has_space(self):
        config = dict(required_mounts=['/volume'], filesystems=[dict(path='/', min_free_bytes=10)])
        with patch.object(guard.os.path, 'ismount', return_value=False), patch.object(
                guard.shutil, 'disk_usage', return_value=SimpleNamespace(free=100)):
            self.assertIn('not mounted', guard.problems(config)[0])

    def test_reserve_boundary(self):
        config = dict(filesystems=[dict(path='/volume', min_free_bytes=100)])
        with patch.object(guard.shutil, 'disk_usage', return_value=SimpleNamespace(free=99)):
            self.assertEqual(len(guard.problems(config)), 1)
        with patch.object(guard.shutil, 'disk_usage', return_value=SimpleNamespace(free=100)):
            self.assertEqual(guard.problems(config), [])

    def test_corrupt_checkpoint_refuses_start(self):
        with patch.object(guard.Path, 'read_text', return_value='{"slots": ['):
            self.assertIn('invalid tournament checkpoint', guard.problems(dict(filesystems=[], state='state.json'))[0])

    def test_supervisor_policy_failure_is_fatal(self):
        with patch.object(guard.Path, 'exists', return_value=True), patch.object(
                guard.Path, 'read_text', return_value='{"filesystems": []}'), patch.object(
                guard, 'problems', return_value=['reserve exhausted']):
            with self.assertRaisesRegex(RuntimeError, 'STORAGE GUARD.*reserve exhausted'):
                guard.require_healthy('/etc/taikyoku/storage.json')


if __name__ == '__main__':
    unittest.main()
