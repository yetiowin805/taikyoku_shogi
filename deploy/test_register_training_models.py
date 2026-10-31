import json
import tempfile
import unittest
from pathlib import Path

import winner_teacher
from register_training_models import merge_bindings


class BindingTests(unittest.TestCase):
    def test_new_agent_is_available_without_rebinding_historical_teachers(self):
        old = {"/old": {"sha256": "unchanged"}}
        current = {**old, "/new": {"sha256": "v4"}}
        self.assertEqual(merge_bindings(old, current), current)
        self.assertNotIn("/new", old)
        self.assertEqual(merge_bindings(current, current), current)
        with self.assertRaises(ValueError):
            merge_bindings(old, {"/old": {"sha256": "different"}})

    def test_symlinked_registration_supports_winner_selection_and_saved_paths(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            actual = root / "actual"
            actual.mkdir()
            alias = root / "alias"
            alias.symlink_to(actual, target_is_directory=True)
            current = {}
            game = dict(result="BlackWins")
            for side in ("black", "white"):
                path = actual / (side + ".json")
                weights = {} if side == "black" else dict(nnue=dict(width=512))
                path.write_text(json.dumps(dict(weights=weights)))
                key = str(alias / path.name)
                current[key] = dict(sha256=side, snapshot=str(path))
                game[side] = dict(name="ab", model=key)
            merged = merge_bindings({}, current)
            for key, binding in current.items():
                self.assertEqual(merged[key], binding)
                self.assertEqual(merged[str(Path(key).resolve())], binding)
            self.assertEqual(merge_bindings(merged, current), merged)
            self.assertEqual(winner_teacher.winner_agent(dict(old=dict(models=merged)), game), game["black"])

    def test_conflicting_aliases_cannot_rebind_a_checkpoint(self):
        with tempfile.TemporaryDirectory() as temp:
            model = Path(temp) / "model.json"
            model.write_text("{}")
            alias = Path(temp) / "alias.json"
            alias.symlink_to(model)
            with self.assertRaisesRegex(ValueError, "Existing teacher binding changed"):
                merge_bindings({str(model): dict(sha256="old")}, {str(alias): dict(sha256="changed")})
