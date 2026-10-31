import copy
import json
import subprocess
from unittest.mock import patch

import champion_teacher as c
from test_tourney_analysis import AnalyzerFixture, a


class ChampionTests(AnalyzerFixture):
    def fixture(self, lead=50):
        model = self.run / "challenger.json"
        a.atomic(model, dict(name="challenger", weights={}))
        binary = self.run / "rater"
        binary.write_text("test")
        config = dict(
            new_sha256="current-sha",
            new_teacher_id="current",
            old=dict(models={str(model): dict(sha256=a.file_digest(model))}),
            auto_champion=dict(ratings_bin=str(binary), ratings_sha256=a.file_digest(binary), margin=50),
        )
        a.atomic(self.run / "analysis/dual-label-config.json", config)
        a.atomic(
            self.run / "state.json",
            dict(
                entrants=[dict(id="current", model="current.json"), dict(id="challenger", model=str(model))],
                slots=[dict(id=1, model_a="current", model_b="challenger", score_a=0, status="done")],
            ),
        )
        fit = dict(status="converged", games=1, ratings=dict(current=1500, challenger=1500 + lead))
        return config, fit

    def execute(self, config, fit):
        with patch.object(a.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, json.dumps(fit), "")):
            return c.check(a, self.run, config)

    def test_tie_holds_but_any_positive_lead_switches(self):
        config, fit = self.fixture(0)
        with patch.object(c.teacher_refresh, "replace") as replace:
            self.execute(config, fit)
            replace.assert_not_called()
        # Same results/policy/incumbent must not rerun the solver.
        with patch.object(a.subprocess, "run", side_effect=AssertionError("repeated fit")):
            c.check(a, self.run, config)
        self.assertEqual(a.read(self.run / "analysis/champion-status.json")["state"], "holding")
        (self.run / "analysis/champion-status.json").unlink()
        fit["ratings"]["challenger"] = 1500.01

        def switch(*args, **kwargs):
            updated = copy.deepcopy(config)
            updated.update(new_sha256="new", new_teacher_id="challenger")
            a.atomic(self.run / "analysis/dual-label-config.json", updated)
            return dict(changed=True, teacher="challenger", backfill_positions=0)

        with patch.object(c.teacher_refresh, "replace", side_effect=switch) as replace:
            result = self.execute(config, fit)
        self.assertEqual(result["new_teacher_id"], "challenger")
        self.assertEqual(replace.call_count, 1)
        self.assertAlmostEqual(a.read(self.run / "analysis/champion-status.json")["lead"], 0.01)

    def test_no_fit_or_mutated_model_keeps_teacher(self):
        config, fit = self.fixture(100)
        fit["status"] = "not_converged"
        with patch.object(c.teacher_refresh, "replace") as replace:
            self.execute(config, fit)
            replace.assert_not_called()
        self.assertEqual(a.read(self.run / "analysis/champion-status.json")["state"], "no_finite_fit")
        (self.run / "analysis/champion-status.json").unlink()
        fit["status"] = "converged"
        (self.run / "challenger.json").write_text("{}")
        self.execute(config, fit)
        self.assertEqual(a.read(self.run / "analysis/champion-status.json")["state"], "error")
        self.assertEqual(a.read(self.run / "analysis/dual-label-config.json")["new_teacher_id"], "current")

    def test_provisional_leader_is_selected_and_retired_leader_excluded(self):
        config, fit = self.fixture(100)
        fit.update(status="separated", components=[["current"], ["challenger"]])
        with patch.object(c.teacher_refresh, "replace", return_value={}) as replace:
            self.execute(config, fit)
            replace.assert_called_once()
        state = a.read(self.run / "state.json")
        state["retired"] = ["challenger"]
        a.atomic(self.run / "state.json", state)
        with patch.object(c.teacher_refresh, "replace") as replace:
            self.execute(config, fit)
            replace.assert_not_called()
        self.assertEqual(a.read(self.run / "analysis/champion-status.json")["leader"], "current")

    def test_error_after_durable_switch_reloads_config(self):
        config, fit = self.fixture(100)

        def interrupted(*args, **kwargs):
            newer = dict(config, new_teacher_id="challenger")
            a.atomic(self.run / "analysis/dual-label-config.json", newer)
            raise RuntimeError("simulated post-commit interruption")

        with patch.object(c.teacher_refresh, "replace", side_effect=interrupted):
            result = self.execute(config, fit)
        self.assertEqual(result["new_teacher_id"], "challenger")

    def test_live_admission_registry_validates_new_teacher_without_restart(self):
        config, fit = self.fixture(100)
        bindings = config["old"]["models"]
        config["old"]["models"] = {}
        a.atomic(self.run / "analysis/config.json", dict(models=bindings))
        with (
            patch.object(a, "validate_model_binding") as validate,
            patch.object(c.teacher_refresh, "replace", return_value={}) as replace,
        ):
            self.execute(config, fit)
            validate.assert_called_once()
            replace.assert_called_once()

    def test_symlinked_checkpoint_in_saved_or_live_registry(self):
        for live_registry in (False, True):
            with self.subTest(live_registry=live_registry):
                config, fit = self.fixture(100)
                model = self.run / "challenger.json"
                directory = self.run / "run-alias"
                if not directory.exists():
                    directory.symlink_to(self.run, target_is_directory=True)
                alias = directory / model.name
                binding = config["old"]["models"].pop(str(model))
                if live_registry:
                    a.atomic(self.run / "analysis/config.json", dict(models={str(alias): binding}))
                else:
                    config["old"]["models"][str(alias)] = binding
                state = a.read(self.run / "state.json")
                state["entrants"][1]["model"] = str(alias)
                a.atomic(self.run / "state.json", state)
                (self.run / "analysis/champion-status.json").unlink(missing_ok=True)
                with (
                    patch.object(a, "validate_model_binding") as validate,
                    patch.object(c.teacher_refresh, "replace", return_value={}) as replace,
                ):
                    self.execute(config, fit)
                    replace.assert_called_once_with(a, self.run, model.resolve(), engine=None)
                    if live_registry:
                        validate.assert_called_once_with(str(model.resolve()), binding)
                    else:
                        validate.assert_not_called()

    def test_symlinked_live_checkpoint_still_requires_valid_frozen_binding(self):
        config, fit = self.fixture(100)
        model = self.run / "challenger.json"
        alias = self.run / "checkpoint-alias.json"
        alias.symlink_to(model)
        binding = config["old"]["models"].pop(str(model))
        a.atomic(self.run / "analysis/config.json", dict(models={str(alias): binding}))
        with (
            patch.object(a, "validate_model_binding", side_effect=ValueError("snapshot changed")),
            patch.object(c.teacher_refresh, "replace") as replace,
        ):
            self.execute(config, fit)
            replace.assert_not_called()
        status = a.read(self.run / "analysis/champion-status.json")
        self.assertEqual(status["state"], "error")
        self.assertEqual(status["error"], "snapshot changed")

    def test_conflicting_checkpoint_aliases_keep_current_teacher(self):
        config, fit = self.fixture(100)
        model = self.run / "challenger.json"
        alias = self.run / "checkpoint-alias.json"
        alias.symlink_to(model)
        config["old"]["models"][str(alias)] = dict(sha256="different-checkpoint")
        with patch.object(c.teacher_refresh, "replace") as replace:
            self.execute(config, fit)
            replace.assert_not_called()
        status = a.read(self.run / "analysis/champion-status.json")
        self.assertEqual(status["state"], "error")
        self.assertIn("conflicting frozen checkpoint aliases", status["error"])
