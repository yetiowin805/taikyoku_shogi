"""Relevant changes run integration checks; ambiguous diffs never suppress them."""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import Mock

import nnue_test_scope as selection


class ScopeTests(unittest.TestCase):
    def test_training_dependencies_select_integration_but_docs_and_engine_do_not(self):
        for path in (
            "training/nnue/pilot_fit.py",
            "training/nnue/new_helper.py",
            "training/nnue/test_v5.py",
            "training/nnue/requirements.txt",
            "deploy/archive_storage.py",
            "deploy/new_dependency.py",
            ".github/workflows/ci.yml",
            "pyproject.toml",
            ".python-version",
        ):
            with self.subTest(path=path):
                self.assertTrue(selection.needs_integration(["README.md", path]))
        self.assertFalse(
            selection.needs_integration(
                ["src/nnue.rs", "Cargo.toml", "training/nnue/README.md", "models/ab-seed.json", "deploy/README.md"]
            )
        )

    def test_pr_and_main_push_compare_their_base_against_tested_checkout(self):
        for event_name, event in (
            ("pull_request", dict(pull_request=dict(base=dict(sha="base")))),
            ("push", dict(before="base")),
        ):
            with self.subTest(event=event_name):
                diff = Mock(return_value=["src/search.rs"])
                self.assertFalse(selection.scope(event_name, event, diff=diff)[0])
                diff.assert_called_once_with("base")
                diff.return_value = ["training/nnue/train.py"]
                self.assertTrue(selection.scope(event_name, event, diff=diff)[0])

    def test_manual_unknown_and_unavailable_diffs_run_integration(self):
        diff = Mock(side_effect=AssertionError("must not inspect a diff"))
        for event in ("workflow_dispatch", "unexpected"):
            self.assertTrue(selection.scope(event, {}, diff=diff)[0])
        for error in (ValueError("missing base"), OSError("git missing"), subprocess.CalledProcessError(1, "git")):
            with self.subTest(error=error):
                self.assertTrue(selection.scope("push", {}, diff=Mock(side_effect=error))[0])
        for base in ("", "0" * 40, "--help"):
            with self.assertRaises(ValueError):
                selection.changed_paths(base)

    def test_real_diff_includes_deleted_side_of_a_rename(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = Path(temp)

            def git(*args):
                return subprocess.check_output(["git", *args], cwd=repo, stderr=subprocess.STDOUT).decode().strip()

            git("init", "-q")
            (repo / "training/nnue").mkdir(parents=True)
            (repo / "training/nnue/old.py").write_text("pass\n")
            git("add", ".")
            git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "fixture")
            base = git("rev-parse", "HEAD")
            git("mv", "training/nnue/old.py", "renamed.py")
            git("-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "rename")
            paths = selection.changed_paths(base, repo=repo)
            self.assertEqual(set(paths), {"training/nnue/old.py", "renamed.py"})
            self.assertTrue(selection.needs_integration(paths))

    def test_ci_output_uses_committed_diff_and_defaults_to_full_on_bad_event(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            base = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=selection.ROOT).decode().strip()
            event, output = root / "event.json", root / "output"
            for contents, name, expected in (
                (json.dumps(dict(before=base)), "push", False),
                ("{}", "workflow_dispatch", True),
                ("invalid JSON", "push", True),
                ("{}", "push", True),
            ):
                with self.subTest(event=name, contents=contents):
                    event.write_text(contents)
                    output.write_text("earlier=value\n")
                    result = subprocess.run(
                        [sys.executable, str(Path(selection.__file__))],
                        env=dict(
                            os.environ, GITHUB_EVENT_PATH=str(event), GITHUB_EVENT_NAME=name, GITHUB_OUTPUT=str(output)
                        ),
                        capture_output=True,
                        text=True,
                        check=True,
                    )
                    self.assertEqual(json.loads(result.stdout)["integration"], expected)
                    self.assertEqual(output.read_text(), f"earlier=value\nintegration={str(expected).lower()}\n")


if __name__ == "__main__":
    unittest.main()
