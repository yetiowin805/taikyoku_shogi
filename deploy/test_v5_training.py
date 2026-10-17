"""A failed stage must return its leases and restore position analysis."""

import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import MagicMock, patch

import v5_training


class RecoveryTest(unittest.TestCase):
    def test_child_failure_restores_service_and_releases_pool(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "run/analysis").mkdir(parents=True)
            config = dict(
                out=str(root / "out"),
                run=str(root / "run"),
                code=str(root),
                repo=str(root),
                cpus=list(range(8)),
                reuse_shards=[],
                code_hashes={},
                input_hashes={},
                python="python",
            )
            path = root / "config.json"
            path.write_text(json.dumps(config))
            pool = MagicMock()
            pool.leases = {1: MagicMock()}
            child = MagicMock()
            child.poll.return_value = 7
            child.returncode = 7
            with (
                patch("sys.argv", ["v5_training", str(path)]),
                patch.object(v5_training.os, "sched_getaffinity", return_value=set(range(8))),
                patch.object(v5_training.signal, "signal"),
                patch.object(v5_training, "Pool", return_value=pool),
                patch.object(v5_training.subprocess, "Popen", return_value=child),
                patch.object(v5_training.subprocess, "run") as service,
            ):
                with self.assertRaisesRegex(RuntimeError, "snapshot failed: 7"):
                    v5_training.main()
            pool.close.assert_called_once()
            self.assertEqual(
                [c.args[0] for c in service.call_args_list],
                [
                    ["systemctl", "stop", "taikyoku-dual-labels.service"],
                    ["systemctl", "start", "taikyoku-dual-labels.service"],
                ],
            )
            self.assertEqual(json.loads((root / "out/status.json").read_text())["state"], "failed")


if __name__ == "__main__":
    unittest.main()
