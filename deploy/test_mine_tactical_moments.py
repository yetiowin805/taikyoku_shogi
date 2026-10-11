import unittest

from mine_tactical_moments import candidates, own_rows


def row(ply, score, depth=3):
    return dict(ply=ply, score=score, depth=depth)


class TacticalMiningTest(unittest.TestCase):
    def test_white_scores_are_normalized_and_gui_ply_is_not_move_number(self):
        game = {
            "moves": [
                {"color": "Black", "eval": 10, "static_eval": 2, "move_number": 1},
                {"color": "White", "eval": -2000, "static_eval": -1000, "move_number": 3},
                {"color": "Black", "eval": -4000, "static_eval": 0, "move_number": 5},
                {"color": "White", "eval": 1000, "static_eval": 500, "move_number": 6},
            ]
        }
        rows = own_rows(game, "White")
        self.assertEqual([(r["ply"], r["score"], r["static"]) for r in rows], [(2, 2000, 1000), (4, -1000, -500)])
        self.assertEqual(candidates(rows)[0][0]["drop"], 3000)

    def test_never_bridge_mate_missing_or_fallback(self):
        for bad in [row(3, 999999), row(3, None), row(3, 0, 0)]:
            self.assertEqual(candidates([row(1, 4000), bad, row(5, -1000)])[0], [])

    def test_local_duplicate_drops_are_one_episode(self):
        chosen, _ = candidates([row(1, 3000), row(3, 1000), row(5, -1000), row(7, -1500), row(9, -1600)])
        self.assertEqual(len(chosen), 1)
        self.assertTrue(chosen[0]["sustained"])

    def test_rebound_is_flagged_not_silently_removed(self):
        chosen, _ = candidates([row(1, 3000), row(3, 0), row(5, 3200), row(7, 3100)])
        self.assertEqual(len(chosen), 1)
        self.assertFalse(chosen[0]["sustained"])

    def test_same_depth_comparison_exposes_depth_oscillation(self):
        chosen, _ = candidates([row(1, -1000, 3), row(3, 2500, 4), row(5, -1100, 3), row(7, 2400, 4), row(9, -1200, 3)])
        self.assertEqual(chosen[0]["same_depth_drop"], 100)
        self.assertEqual(chosen[0]["depth_change"], -1)


if __name__ == "__main__":
    unittest.main()
