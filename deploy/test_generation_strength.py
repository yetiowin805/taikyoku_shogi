"""Numerical checks for the optional offline NumPyro analysis environment."""

import importlib.util
import itertools
import unittest


@unittest.skipUnless(importlib.util.find_spec("numpyro"), "offline analysis requires NumPyro")
class GenerationStrengthTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import numpy

        try:
            from deploy import generation_strength
        except ModuleNotFoundError:
            import generation_strength

        cls.np = numpy
        cls.analysis = generation_strength

    def test_best_of_k_matches_enumerated_subsets(self):
        for n in range(5, 10):
            values = self.np.arange(n, dtype=float) ** 2
            for k in [1, 3, 5]:
                exact = self.np.mean([max(v) for v in itertools.combinations(values, k)])
                self.assertAlmostEqual(float(self.analysis.expected_best(values, k)), exact)

    def test_standard_normal_quadrature_moments(self):
        for points in [9, 15, 25]:
            nodes, log_weights = self.analysis.quadrature(points)
            weights = self.np.exp(log_weights)
            self.assertAlmostEqual(float(weights.sum()), 1)
            self.assertAlmostEqual(float(weights @ nodes), 0)
            self.assertAlmostEqual(float(weights @ (nodes**2)), 1)
            self.assertAlmostEqual(float(weights @ (nodes**4)), 3)

    def test_exact_color_probabilities_and_joint_pair_probability(self):
        np = self.np
        obs = {
            "a": np.array([[0, 1]]),
            "b": np.array([[1, 0]]),
            "color": np.array([[1, -1]]),
            "y": np.array([[0, 2]]),
            "mask": np.ones((1, 2), dtype=int),
        }
        samples = {
            "theta": np.zeros((2, 10, 2)),
            "black": np.full((2, 10), np.log(3)),
            "log_nu": np.full((2, 10), -40.0),
        }
        probabilities, joint = self.analysis.predictive(samples, obs)
        np.testing.assert_allclose(probabilities[0, :, 0], [0.75, 0.25])
        np.testing.assert_allclose(joint, np.log(0.75 * 0.75))
        obs["mask"][0, 1] = 0
        _, joint = self.analysis.predictive(samples, obs)
        np.testing.assert_allclose(joint, np.log(0.75))

    def test_davidson_tie_probability(self):
        np = self.np
        obs = {k: np.zeros((1, 1), dtype=int) for k in ["a", "b", "color", "y"]}
        obs["mask"] = np.ones((1, 1), dtype=int)
        samples = {"theta": np.zeros((2, 10, 1)), "black": np.zeros((2, 10)), "log_nu": np.full((2, 10), np.log(2))}
        probabilities, _ = self.analysis.predictive(samples, obs)
        np.testing.assert_allclose(probabilities[0, 0], [0.25, 0.5, 0.25])

    def test_reversed_player_order_preserves_pair_block(self):
        slot = {
            "id": 4,
            "model_a": "a",
            "model_b": "b",
            "start_seed": 10,
            "start_mode": "opening",
            "a_is_black": True,
            "score_a": 1.0,
        }
        reverse = {**slot, "id": 7, "model_a": "b", "model_b": "a", "a_is_black": False, "score_a": 0.0}
        arrays, ids = self.analysis.observations([slot, reverse], ["a", "b"])
        self.assertEqual(ids.tolist(), [[4, 7]])
        self.assertEqual(arrays["color"].tolist(), [[1, -1]])
        self.assertEqual(arrays["y"].tolist(), [[0, 2]])

    def test_generation_labels_and_field_reference_are_generic(self):
        import json
        import tempfile
        from pathlib import Path

        names = ["plain-a", "plain-b", "NNUE_W64_v9", "NNUE_W64_v9_FAST"]
        payload = {
            "state": {"entrants": [{"id": n} for n in names], "slots": [], "retired": []},
            "fit": {
                "retirement": {
                    "generations": {
                        "old": {"members": names[:2]},
                        "network9": {"members": names[2:3]},
                        "new_search": {"members": names[3:]},
                    }
                }
            },
        }
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "snapshot.json"
            path.write_text(json.dumps(payload))
            design, slots, payload = self.analysis.load_design(path)
        self.assertEqual(len(design["variant_base"]), 1)
        self.assertEqual(design["waves"], ["new_search"])
        np = self.np
        samples = {
            "theta": np.arange(80 * 4, dtype=float).reshape(2, 40, 4) / 100,
            "black": np.zeros((2, 40)),
            "sigma_open": np.zeros((2, 40)),
        }
        weights = np.full(80, 1 / 80)
        result = self.analysis.report(samples, design, slots, payload, weights)
        shifted = self.analysis.report({**samples, "theta": samples["theta"] + 12}, design, slots, payload, weights)
        np.testing.assert_allclose(
            [r["strength"]["mean"] for r in result["agents"]], [r["strength"]["mean"] for r in shifted["agents"]]
        )
        self.assertAlmostEqual(np.mean([r["strength"]["mean"] for r in result["agents"]]), 1500)
        self.assertEqual(result["equal_k"], 1)


if __name__ == "__main__":
    unittest.main()
