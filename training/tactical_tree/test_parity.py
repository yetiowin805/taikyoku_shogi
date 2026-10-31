import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from parity import finite_metrics, run
from parity_prepare import select_cases

from run import sha


class ParityTests(unittest.TestCase):
    def test_resume_preserves_completed_requests(self):
        calls = []
        fail = [True]

        class FakeOracle:
            ready = {"search_config": "fixed", "actual": [], "history_len": 99}

            def __init__(self, *_args, **_kwargs):
                pass

            def call(self, req, receive):
                calls.append(req)
                if req["op"] == "search":
                    if req["depth"] == 2 and fail[0]:
                        raise InterruptedError("explicit interruption")
                    receive(dict(event="iteration", depth=req["depth"], score_black=123, best={"route": 1}))
                    return dict(event="complete", completed_depth=req["depth"], aborted=False, q_nodes=7)
                return dict(event="complete", turn=-1, static_black=123, flipped_static_black=100)

            def close(self):
                pass

        with tempfile.TemporaryDirectory() as tmp, patch("parity.Oracle", FakeOracle):
            out = Path(tmp)
            game = out / "game.json"
            game.write_text("{}")
            case = dict(id="case", game=str(game), sha256=sha(game), maximum_depth=2)
            config = dict(out=tmp, binary_sha256="test", models={"model": dict(path="test", identities={})})
            with self.assertRaises(InterruptedError):
                run(config, case, "model")
            before = json.loads((out / "case/model/summary.json").read_text())
            self.assertFalse(before["finished"])
            self.assertIn("1", before["records"])
            fail[0] = False
            run(config, case, "model")
            after = json.loads((out / "case/model/summary.json").read_text())
            self.assertTrue(after["finished"])
            self.assertEqual(after["records"]["1"], before["records"]["1"])
            self.assertEqual(sum(r["op"] == "search" and r["depth"] == 1 for r in calls), 1)

    def test_detrended_gap_and_mates(self):
        records = {str(d): dict(score_black=s, best=d % 2) for d, s in enumerate([712, -6776, 695, -6797, 647], 1)}
        m = finite_metrics(records, -1)
        self.assertEqual(m["odd_even_offset_black"], 7481.5)
        self.assertEqual(m["odd_even_offset_root"], -7481.5)
        self.assertEqual(m["same_parity_black"], {"3": -17, "4": -21, "5": -48})
        records["4"]["score_black"] = 999999
        self.assertIsNone(finite_metrics(records, 1)["odd_even_offset_black"])
        self.assertEqual(finite_metrics(records, 1)["mate_depths"], [4])

    def test_selection_is_unique_reproducible_and_score_independent(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            slots = []
            for slot in range(1300, 1320):
                game = root / f"{slot}.json"
                game.write_text(json.dumps(dict(moves=[dict(color="Black", eval=0)] * 200)))
                slots.append(dict(id=slot, status="done", model_a="NNUE_W512_v4", model_b="HCE", game_path=str(game)))
            (root / "state.json").write_text(json.dumps(dict(slots=slots)))
            a = select_cases(root, {1301, 1302}, 42)
            for slot in slots:
                Path(slot["game_path"]).write_text(json.dumps(dict(moves=[dict(color="Black", eval=1000000)] * 200)))
            self.assertEqual(a, select_cases(root, {1301, 1302}, 42))
            self.assertEqual(len({r["slot"] for r in a}), 12)
            self.assertFalse({r["slot"] for r in a} & {1301, 1302})
            self.assertEqual([sum(r["phase"] == p for r in a) for p in ("early", "middle", "late")], [4, 4, 4])
            self.assertEqual(sum(r["maximum_depth"] == 5 for r in a), 3)


if __name__ == "__main__":
    unittest.main()
