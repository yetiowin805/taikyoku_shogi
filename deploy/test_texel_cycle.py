import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


class TexelCycleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "deploy").mkdir()
        shutil.copy2(Path(__file__).with_name("run_texel_cycle.sh"), self.root / "deploy/run_texel_cycle.sh")
        self.games = self.root / "games with 'quotes'"
        self.games.mkdir()
        (self.games / "game.json").write_text('{"moves":[]}')
        self.features = self.root / "features"
        self.features.mkdir()
        (self.features / "old.json").write_text("old features")
        self.calls = self.root / "calls"
        self.notice = self.root / "notice.json"
        self.engine = self.root / "engine"
        self.engine.write_text("""#!/usr/bin/env python3
import os
import sys
from pathlib import Path
with Path(os.environ['CALLS']).open('a') as output:
    output.write(sys.argv[1] + '\\n')
raise SystemExit(int(os.environ.get('FEAT_RC' if sys.argv[1] == 'featurize' else 'FIT_RC', '0')))
""")
        self.engine.chmod(0o755)
        self.notify = self.root / "notify"
        self.notify.write_text("""#!/usr/bin/env python3
import json
import os
import sys
from pathlib import Path
Path(os.environ['NOTICE']).write_text(json.dumps(sys.argv[1:]))
raise SystemExit(int(os.environ.get('NOTIFY_RC', '0')))
""")
        self.notify.chmod(0o755)

    def run_cycle(self, **overrides):
        env = dict(
            os.environ, BIN=str(self.engine), NOTIFY=str(self.notify), CALLS=str(self.calls), NOTICE=str(self.notice)
        )
        env["ACTIVITY_STAMP"] = str(self.root / "data/run/activity")
        env.update(overrides)
        return subprocess.run(
            [
                "bash",
                str(self.root / "deploy/run_texel_cycle.sh"),
                "--games-dir",
                str(self.games),
                "--features",
                str(self.features),
            ],
            env=env,
            text=True,
            capture_output=True,
            timeout=10,
        )

    def test_failed_extraction_never_fits_or_announces_success(self):
        result = self.run_cycle(FEAT_RC="42")
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), ["featurize"])
        self.assertIn("texel-fit did not start", result.stdout)
        self.assertIn("featurize failed", result.stderr)
        self.assertIn("FAILED", json.loads(self.notice.read_text())[0])
        self.assertEqual((self.features / "old.json").read_text(), "old features")

    def test_failure_with_no_feature_directory_still_reports_the_original_status(self):
        shutil.rmtree(self.features)
        result = self.run_cycle(FEAT_RC="42", NOTIFY_RC="7")
        self.assertEqual(result.returncode, 42)
        self.assertIn("FAILED", json.loads(self.notice.read_text())[0])
        self.assertIn("notification failed", result.stderr)

    def test_fit_failure_is_preserved(self):
        result = self.run_cycle(FIT_RC="43")
        self.assertEqual(result.returncode, 43)
        self.assertEqual(self.calls.read_text().splitlines(), ["featurize", "texel-fit"])
        self.assertIn("FAILED", json.loads(self.notice.read_text())[0])

    def test_success_announces_success_only_after_both_stages(self):
        result = self.run_cycle()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls.read_text().splitlines(), ["featurize", "texel-fit"])
        self.assertIn("training OK", json.loads(self.notice.read_text())[0])
        self.assertTrue((self.root / "data/run/activity").exists())

    def test_logging_failure_cannot_report_success(self):
        tools = self.root / "tools"
        tools.mkdir()
        tee = tools / "tee"
        tee.write_text("#!/bin/sh\ncat\nexit 5\n")
        tee.chmod(0o755)
        result = self.run_cycle(PATH=f"{tools}:{os.environ['PATH']}")
        self.assertEqual(result.returncode, 5)
        self.assertIn("training log failed", result.stderr)
        self.assertIn("FAILED", json.loads(self.notice.read_text())[0])


if __name__ == "__main__":
    unittest.main()
