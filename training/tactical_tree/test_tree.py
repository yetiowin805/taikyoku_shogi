import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import supervise
from audit import compare
from rollout import evaluate, summarize
from supervise import BoundaryLeases, claim
from tree import Tree, key

from run import STOP


def mv(i):
    return {"from": {"file": i, "rank": 0}, "to": {"file": i, "rank": 1}, "promoted": False, "data": "Standard"}


def inspect(tree, node, turn=1, terminal=None, moves=None):
    tree.inspect(
        node,
        dict(
            legal=[dict(move=m) for m in moves or []],
            turn=turn,
            terminal=terminal,
            static_black=0,
            position_hash=str(node),
            history_len=node,
            draw_counter=0,
            repetitions=1,
        ),
    )


def search(tree, node, depth, score, best, candidates=None):
    tree.iteration(
        node,
        dict(depth=depth, score_black=score, best=best, candidates=candidates or [dict(move=best, score_black=score)]),
    )


class TreeTest(unittest.TestCase):
    def test_new_reply_reassigns_ancestors_and_parity_does_not_prove(self):
        a, b, c = mv(1), mv(2), mv(3)
        t = Tree([], 0)
        inspect(t, 0, moves=[a, b])
        search(t, 0, 2, 100, a, [dict(move=a, score_black=100), dict(move=b, score_black=50)])
        x = t.add(0, a)
        inspect(t, x, turn=-1, moves=[c])
        search(t, x, 2, 100, c)
        y = t.add(x, c)
        inspect(t, y, terminal=-1)
        t.backup()
        self.assertEqual(t.nodes[0]["selected"], b)
        self.assertEqual(t.nodes[0]["value"], 50)
        self.assertEqual((t.nodes[0]["proof_low"], t.nodes[0]["proof_high"]), (-1, 1))
        search(t, 0, 3, -500, b)
        self.assertEqual(t.parity(0), 600)

    def test_full_coverage_proves_hidden_win_and_can_resume(self):
        moves = [mv(i) for i in range(5)]
        t = Tree([], 0)
        inspect(t, 0, moves=moves)
        for i in range(5):
            a = t.coverage()
            inspect(t, a["node"], turn=-1, terminal=1 if i == 4 else -1)
            t = Tree([], 0, json.loads(json.dumps(t.data)))
        t.backup()
        self.assertEqual(t.nodes[0]["proof_low"], 1)
        self.assertEqual(t.nodes[0]["proof_high"], 1)
        self.assertIsNone(t.coverage())

    def test_fairness_reaches_descendants_even_with_misleading_hints(self):
        a, b = mv(1), mv(2)
        t = Tree([], 0)
        inspect(t, 0, moves=[a, b])
        search(t, 0, 3, 1000000, a)
        x = t.coverage()["node"]
        inspect(t, x, turn=-1, moves=[a, b])
        y = t.coverage()["node"]
        inspect(t, y, turn=-1, terminal=0)
        self.assertEqual(t.nodes[t.coverage()["node"]]["parent"], x)

    def test_terminal_draw_is_distinct_from_unknown(self):
        t = Tree([], 0)
        inspect(t, 0, terminal=0)
        t.backup()
        self.assertEqual(t.nodes[0]["proof_low"], 0)
        self.assertEqual(t.nodes[0]["value"], 0)

    def test_terminal_pv_does_not_repeat_zero_cost_searches(self):
        a = mv(1)
        t = Tree([], 0)
        inspect(t, 0, moves=[a])
        search(t, 0, 3, 1000000, a)
        child = t.add(0, a)
        inspect(t, child, terminal=1)
        t.backup()
        self.assertIsNone(t.descend(0))
        self.assertIsNone(t.coverage())

    def test_route_data_identity_and_stale_iteration(self):
        a = mv(1)
        b = dict(a, data={"TwoStep": {"intermediate": {"file": 5, "rank": 5}}})
        self.assertNotEqual(key(a), key(b))
        t = Tree([], 0)
        search(t, 0, 5, 200, a)
        search(t, 0, 2, -100, b)
        self.assertEqual(t.nodes[0]["best"], a)

    def test_lease_respects_existing_player_and_does_not_touch_demand(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp)
            (p / "analysis-demand.json").write_text('{"assigned":[1,3],"pid":123}')
            first = claim(p, [4, 5], {})
            self.assertEqual(first[0], 4)
            second = claim(p, [4], {})
            self.assertIsNone(second)
            self.assertIsNone(claim(p, [0, 2], {}))
            self.assertEqual(json.loads((p / "analysis-demand.json").read_text())["pid"], 123)
            first[1].close()
            again = claim(p, [4], {})
            self.assertEqual(again[0], 4)
            again[1].close()

    def test_boundary_waiter_never_interrupts_game_and_limits_claims(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp)
            game = claim(p, [4], {})
            waiting = BoundaryLeases(p, [4], 1)
            self.assertIsNone(waiting.take())
            game[1].close()
            acquired = waiting.ready.get(timeout=2)
            self.assertEqual(acquired[0], 4)
            self.assertIsNone(claim(p, [4], {}))
            waiting.close()
            acquired[1].close()
            for thread in waiting.threads:
                thread.join(timeout=2)
            again = claim(p, [4], {})
            self.assertIsNotNone(again)
            again[1].close()


class RolloutTest(unittest.TestCase):
    def tearDown(self):
        STOP.clear()

    def test_full_paths_equal_horizons_and_resume(self):
        prefix = [mv(99)]
        rows = [dict(move=mv(i), played=i == 1, continuation=[], steps={}) for i in (1, 2)]

        class Oracle:
            calls = []

            def call(self, request, receive):
                self.calls.append(request)
                path = request["path"]
                score = 100 if path[1] == mv(1) else 300
                receive(dict(event="iteration", depth=2, score_black=-score, best=mv(3)))
                receive(dict(event="iteration", depth=3, score_black=score, best=mv(4)))
                return dict(event="complete", terminal=None)

        oracle = Oracle()
        evaluate(oracle, rows, prefix, lambda: None)
        self.assertEqual(len(oracle.calls), 18)
        self.assertEqual([len(r["path"]) for r in oracle.calls], [i for i in range(2, 11) for _ in range(2)])
        self.assertTrue(all(r["path"][0] == prefix[0] for r in oracle.calls))
        results = summarize(rows, -1)
        self.assertEqual(results[1]["horizons"]["4"]["gain"], -200)
        self.assertEqual(results[1]["horizons"]["8"]["gain"], -200)
        self.assertEqual(results[1]["shallow_steps"], 0)
        # Completed endpoints are reused, not re-played or overwritten.
        evaluate(oracle, json.loads(json.dumps(rows)), prefix, lambda: None)
        self.assertEqual(len(oracle.calls), 18)

    def test_terminal_is_propagated_and_missing_depths_are_not_equal_work(self):
        rows = [dict(move=mv(i), played=i == 1, continuation=[], steps={}) for i in (1, 2)]

        class Oracle:
            def call(self, request, receive):
                if request["path"][0] == mv(1):
                    return dict(event="complete", terminal=-1)
                receive(dict(event="iteration", depth=2, score_black=50, best=mv(3)))
                return dict(event="complete", terminal=None)

        evaluate(Oracle(), rows, [], lambda: None)
        results = summarize(rows, 1)
        self.assertEqual(results[0]["horizons"]["8"]["endpoint"]["terminal"], -1)
        self.assertEqual(results[1]["shallow_steps"], 9)
        a = dict(played=True, records={"3": dict(score_black=0)})
        b = dict(move=mv(2), records={"2": dict(score_black=500)})
        self.assertEqual(compare([a, b], 1)[0]["depths"], [])

    def test_interrupted_iteration_is_durable_before_a_reply_is_chosen(self):
        rows = [dict(move=mv(1), played=True, continuation=[], steps={})]
        saved = []

        class Oracle:
            def call(self, request, receive):
                receive(dict(event="iteration", depth=2, score_black=20, best=mv(2)))
                raise InterruptedError

        with self.assertRaises(InterruptedError):
            evaluate(Oracle(), rows, [], lambda: saved.append(json.loads(json.dumps(rows))))
        self.assertEqual(saved[-1][0]["steps"]["0"]["records"]["2"]["score_black"], 20)
        self.assertEqual(saved[-1][0]["continuation"], [])

    def test_parity_disagreement_does_not_count_as_supported_improvement(self):
        a = dict(played=True, records={"2": dict(score_black=0), "3": dict(score_black=100)})
        b = dict(move=mv(2), records={"2": dict(score_black=2000), "3": dict(score_black=-100)})
        result = compare([a, b], 1)[0]
        self.assertEqual(result["robust_gain"], -200)
        self.assertFalse(result["supported_improvement"])


class SupervisorTest(unittest.TestCase):
    def test_elapsed_time_never_terminates_a_healthy_job(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = dict(
                out=str(root),
                control=str(root),
                workers=1,
                lease_indices=[4],
                cpus=list(range(8)),
                identities={},
                minimum_free_bytes=0,
                seed=1,
                wall_seconds=1,
                cases=[dict(id="case")],
            )
            path = root / "config.json"
            path.write_text(json.dumps(config))
            lease = (root / "lease").open("w")
            waiters = mock.Mock()
            waiters.take.side_effect = [(4, lease)]
            launched = []

            class Process:
                pid = 123
                returncode = 0

                def __init__(self, cmd, **kwargs):
                    self.polls = 0
                    strategy = cmd[-1] if "--strategy" in cmd else "audit"
                    out = root / "case" / strategy
                    out.mkdir(parents=True, exist_ok=True)
                    filename = "audit.json" if strategy == "audit" else "summary.json"
                    (out / filename).write_text(json.dumps(dict(finished=True, work_complete=True)))
                    launched.append(strategy)

                def poll(self):
                    self.polls += 1
                    return None if self.polls == 1 else 0

                def terminate(self):
                    raise AssertionError("healthy worker must not be terminated because time elapsed")

            with (
                mock.patch.object(supervise, "BoundaryLeases", return_value=waiters),
                mock.patch.object(supervise, "active_admission", return_value=False),
                mock.patch.object(supervise.os, "sched_getaffinity", return_value=set(range(8))),
                mock.patch.object(supervise.subprocess, "Popen", side_effect=Process),
                mock.patch.object(supervise.time, "sleep"),
                mock.patch.object(supervise.time, "time", side_effect=range(0, 100000000, 100000)),
            ):
                supervise.run(path)
            self.assertEqual(len(launched), 5)
            self.assertEqual(json.loads((root / "status.json").read_text())["state"], "completed")


if __name__ == "__main__":
    unittest.main()
