"""Metadata-only generation migrations preserve tournament evidence."""

import copy
import json
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import upgrade_tournament_scheduler as upgrade
from upgrade_tournament_scheduler import generation_transition


class GenerationTransitionTests(unittest.TestCase):
    def setUp(self):
        self.state = dict(
            entrants=[
                dict(id="v6", model="base.json", generation="v6"),
                dict(id="new_search", model="search.json", generation="search"),
                dict(id="old_search", model="old.json", generation="search"),
            ],
            slots=[dict(id=7, status="done", model_a="v6", model_b="new_search", score_a=1)],
            retired=["old_search"],
            runtime_control=dict(assignments_paused=True),
        )

    def test_only_selected_metadata_changes_and_retry_is_idempotent(self):
        original = copy.deepcopy(self.state)
        bindings = dict(new_search="v6_search")
        changed = generation_transition(self.state, bindings)
        self.assertEqual(self.state, original)
        self.assertEqual(changed["entrants"][1]["generation"], "v6_search")
        self.assertEqual(changed["entrants"][0], original["entrants"][0])
        self.assertEqual(changed["entrants"][2], original["entrants"][2])
        self.assertEqual(changed["slots"], original["slots"])
        self.assertEqual(changed["retired"], original["retired"])
        self.assertEqual(changed["runtime_control"], original["runtime_control"])
        self.assertEqual(generation_transition(changed, bindings), changed)

    def test_bad_labels_and_unknown_entrants_fail_before_any_change(self):
        original = copy.deepcopy(self.state)
        for bindings in (
            None,
            [],
            {},
            {"missing": "v6_search"},
            {"new_search": ""},
            {"new_search": " v6"},
            {"new_search": 4},
        ):
            with self.subTest(bindings=bindings), self.assertRaises(ValueError):
                generation_transition(self.state, bindings)
            self.assertEqual(self.state, original)

    def test_upgrade_publishes_metadata_only_after_drain_and_stop(self):
        with tempfile.TemporaryDirectory() as temp:
            run = Path(temp)
            control = run / "analysis"
            control.mkdir()
            binary = run / "coordinator"
            binary.write_bytes(b"unchanged coordinator")
            mapping = run / "mapping.json"
            mapping.write_text(json.dumps(dict(new_search="v6_search")))
            (run / "state.json").write_text(json.dumps(self.state))
            (control / "manifest.json").write_text(json.dumps(dict(entrants=self.state["entrants"])))
            (control / "active-engine.json").write_text("{}")
            (control / "config.json").write_text(
                json.dumps(dict(cpus=list(range(8)), sidecar="none", command=[str(binary)]))
            )
            steps = []

            def drain(*args):
                steps.append("drain")

            def stop(*args):
                self.assertEqual(steps, ["drain"])
                self.assertEqual(json.loads((run / "state.json").read_text()), self.state)
                steps.append("stop")

            def service(action, name):
                self.assertEqual(action, "start")
                self.assertEqual(steps, ["drain", "stop"])
                changed = json.loads((run / "state.json").read_text())
                self.assertEqual(changed["entrants"][1]["generation"], "v6_search")
                self.assertEqual(changed["slots"], self.state["slots"])
                self.assertEqual(json.loads((control / "manifest.json").read_text())["entrants"], changed["entrants"])
                steps.append("start")

            args = SimpleNamespace(
                run=run,
                binary=binary,
                coordinator="coordinator.service",
                replace_service=[],
                external_service=[],
                initial_time_ms=None,
                increment_ms=None,
                generation_map=mapping,
            )
            with (
                patch.object(upgrade, "drain", side_effect=drain),
                patch.object(upgrade, "stop_idle_coordinator", side_effect=stop),
                patch.object(upgrade, "service", side_effect=service),
                patch.object(upgrade.subprocess, "check_output", return_value="1\n"),
                patch.object(upgrade.tasks, "ready"),
            ):
                upgrade.upgrade(args)
            self.assertEqual(steps, ["drain", "stop", "start"])
            backups = list(control.glob("before-scheduler-upgrade-*/state.json"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(json.loads(backups[0].read_text()), self.state)


if __name__ == "__main__":
    unittest.main()
