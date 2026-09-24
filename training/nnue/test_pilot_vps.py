import json
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import pilot_vps


class HandoffTest(unittest.TestCase):
    def test_pause_and_restore_preserves_process_identity(self):
        child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'])
        try:
            config=dict(analyzer_pid=child.pid,analyzer_identity=pilot_vps.identity(child.pid),control='/tmp/unused-pilot-test')
            pilot_vps.pause_at_boundary(config)
            self.assertTrue(Path(f'/proc/{child.pid}/stat').read_text().split(') ',1)[1].startswith('T'))
            with tempfile.TemporaryDirectory() as d, patch.object(pilot_vps,'ROOT',Path(d)):
                pilot_vps.restore(config)
                self.assertEqual(json.loads((Path(d)/'restoration.json').read_text())['state'],'analyzer_resumed')
                self.assertIsNone(child.poll())
                with self.assertRaises(RuntimeError):
                    pilot_vps.restore(dict(config,analyzer_identity='wrong'))
        finally:
            child.send_signal(signal.SIGCONT)
            child.terminate();child.wait()

    def test_timeout_reaps_command(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(TimeoutError):
                pilot_vps.command([sys.executable,'-c','import time;time.sleep(60)'],Path(d)/'log',.1)

if __name__=='__main__':unittest.main()
