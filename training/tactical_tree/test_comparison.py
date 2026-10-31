import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import comparison
import leader
import supervise
from comparison import CpuClock, action, observe, recommendations
from comparison_audit import endpoint_pair, proposal_at
from test_tree import inspect, mv, search
from tree import Tree


class ComparisonTest(unittest.TestCase):
    def test_interrupted_search_resumes_with_completed_iteration_and_provenance(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            case = dict(id="case", start_ply=1, end_ply=2)
            config = dict(out=str(root), identities={}, checkpoints=[1, 2])

            class Clock:
                def __init__(self):
                    self.value = 0

                def read(self):
                    self.value += 0.01
                    return self.value

            class Oracle:
                interrupt = True
                ready = dict(actual=[mv(1)])
                proc = mock.Mock(pid=1)

                def __init__(self, *_):
                    pass

                def close(self):
                    pass

                def call(self, request, receive):
                    if request["op"] == "inspect":
                        return dict(
                            legal=[dict(move=mv(1))],
                            terminal=None,
                            turn=1,
                            static_black=0,
                            position_hash="a",
                            history_len=0,
                            draw_counter=0,
                            repetitions=1,
                        )
                    receive(
                        dict(
                            event="iteration",
                            depth=request["depth"],
                            score_black=123,
                            best=mv(1),
                            candidates=[dict(move=mv(1), score_black=123)],
                        )
                    )
                    if self.interrupt:
                        raise InterruptedError("explicit interruption")
                    return dict(event="complete")

            with mock.patch.object(comparison, "CpuClock", Clock), mock.patch.object(comparison, "Oracle", Oracle):
                with self.assertRaises(InterruptedError):
                    comparison.run(config, case, "uniform")
                path = root / "case/uniform/tree.json"
                saved = json.loads(path.read_text())
                self.assertEqual(saved["nodes"][0]["records"]["3"]["score_black"], 123)
                self.assertFalse(saved["finished"])
                Oracle.interrupt = False
                comparison.run(config, case, "uniform")
                resumed = json.loads(path.read_text())
                self.assertTrue(resumed["finished"])
                self.assertEqual(resumed["resumes"], 1)
                self.assertEqual(resumed["nodes"][0]["records"]["3"]["score_black"], 123)

    def test_baselines_do_not_inherit_child_recommendations_or_expand_branches(self):
        a, b = mv(1), mv(2)
        tree = Tree([a], 1)
        root, child = tree.spine()
        inspect(tree, root, moves=[a, b])
        inspect(tree, child, turn=-1)
        search(tree, root, 3, 100, a, [dict(move=a, score_black=100), dict(move=b, score_black=50)])
        search(tree, child, 3, -1000, b)
        case = dict(start_ply=10)
        self.assertEqual(recommendations(tree, "branches", case)[0]["best"], b)
        for strategy in ("uniform", "adaptive"):
            self.assertEqual(recommendations(tree, strategy, case)[0]["best"], a)
            for _ in range(22):
                tree.data["actions"] += 1
                self.assertIn(action(tree, strategy)["node"], (root, child))
        self.assertEqual(len(tree.nodes), 2)

    def test_checkpoints_exclude_late_results_and_survive_resume(self):
        data = dict(snapshots={})
        points = [60, 180]
        observe(data, 58, [dict(best="old")], points)
        observe(data, 63, [dict(best="late")], points)
        self.assertEqual(data["snapshots"]["60"]["recommendations"], [dict(best="old")])
        self.assertEqual(data["snapshots"]["60"]["cpu_seconds"], 58)
        data = json.loads(json.dumps(data))
        observe(data, 179, [dict(best="new")], points)
        observe(data, 185, [dict(best="too-new")], points)
        self.assertEqual(data["snapshots"]["180"]["recommendations"], [dict(best="new")])
        self.assertEqual(data["snapshots"]["60"]["recommendations"], [dict(best="old")])

    def test_cpu_clock_counts_worker_and_driver_not_waiting_wall_time(self):
        fields = ["0"] * 30
        fields[11:13] = ["300", "40"]
        with (
            mock.patch("comparison.time.process_time", side_effect=[10, 10.5]),
            mock.patch("comparison.os.sysconf", return_value=100),
            mock.patch.object(Path, "read_text", return_value="123 (worker with spaces) " + " ".join(fields)),
        ):
            clock = CpuClock()
            clock.pid = 123
            self.assertAlmostEqual(clock.read(), 3.9)

    def test_common_depth_endpoints_sign_missing_and_terminal_evidence(self):
        def row(records):
            return dict(steps={"4": dict(done=True, records={str(d): dict(score_black=v) for d, v in records.items()})})

        a, b = row({2: 100, 3: 1000}), row({2: 20})
        result = endpoint_pair(a, b, 4, -1)
        self.assertEqual((result["depth"], result["gain"]), (2, -80))
        self.assertIsNone(endpoint_pair(a, b, 8, 1)["gain"])
        a.update(terminal=1, terminal_after=2)
        self.assertIsNone(endpoint_pair(a, b, 4, 1)["gain"])
        b.update(terminal=-1, terminal_after=3)
        self.assertEqual(endpoint_pair(a, b, 4, 1)["wdl_gain"], 2)

    def test_missing_proposal_is_not_a_default_move(self):
        s = dict(snapshots={"60": dict(recommendations=[dict(ply=10, best=mv(1))])})
        self.assertIsNone(proposal_at(s, 60, 11))


class LeaderTest(unittest.TestCase):
    def test_small_lead_switches_next_case_but_never_a_partly_finished_case(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            entrants = []
            for name in ("a", "b"):
                path = root / (name + ".json")
                path.write_text(json.dumps(dict(name=name, weights={})))
                entrants.append(dict(id=name, model=str(path)))
            state = dict(entrants=entrants, retired=[])
            fit = dict(status="converged", ratings=dict(a=1600.1, b=1600.0))
            (root / "state.json").write_text(json.dumps(state))
            rating_path = root / "order-neutral-ratings.json"
            rating_path.write_text(json.dumps(fit))
            config = dict(out=str(root / "out"), leader_run=str(root), identities={})
            first = leader.bind_case(config, dict(id="first"))
            self.assertEqual(json.loads(first.read_text())["leader"]["id"], "a")
            fit["ratings"]["b"] = 1600.2
            rating_path.write_text(json.dumps(fit))
            self.assertEqual(leader.bind_case(config, dict(id="first")), first)
            self.assertEqual(json.loads(first.read_text())["leader"]["id"], "a")
            second = leader.bind_case(config, dict(id="second"))
            self.assertEqual(json.loads(second.read_text())["leader"]["id"], "b")
            frozen = Path(json.loads(first.read_text())["model"])
            frozen.write_text("changed")
            with self.assertRaisesRegex(ValueError, "immutable input changed"):
                leader.bind_case(config, dict(id="first"))

    def test_invalid_ratings_and_retired_agents(self):
        state = dict(entrants=[dict(id="a"), dict(id="b")], retired=["a"])
        self.assertEqual(
            leader.select_leader(state, dict(status="disconnected", ratings=dict(a=1700, b=1500)))[0]["id"], "b"
        )
        with self.assertRaises(ValueError):
            leader.select_leader(state, dict(status="converged", ratings=dict(a=float("nan"))))


class MaintenanceTest(unittest.TestCase):
    def test_maintenance_finishes_work_releases_cpu_and_resumes(self):
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
                cases=[dict(id="case")],
            )
            path = root / "config.json"
            path.write_text(json.dumps(config))
            leases = []

            def waiter(*_):
                lease = (root / "lease").open("a")
                leases.append(lease)
                result = mock.Mock()
                result.take.return_value = (4, lease)
                return result

            class Process:
                pid, returncode = 1, 0

                def __init__(self, cmd, **_):
                    self.polls = 0
                    strategy = cmd[-1] if "--strategy" in cmd else "audit"
                    out = root / "case" / strategy
                    out.mkdir(parents=True, exist_ok=True)
                    name = "audit.json" if strategy == "audit" else "summary.json"
                    (out / name).write_text(json.dumps(dict(finished=True, work_complete=True)))

                def poll(self):
                    self.polls += 1
                    return None if self.polls == 1 else 0

                def terminate(self):
                    raise AssertionError("maintenance must not terminate a healthy process")

            busy = iter([False, True, True, False])
            with (
                mock.patch.object(supervise, "BoundaryLeases", side_effect=waiter),
                mock.patch.object(supervise, "active_admission", return_value=False),
                mock.patch.object(supervise, "allocation_busy", side_effect=lambda _: next(busy, False)),
                mock.patch.object(supervise.os, "sched_getaffinity", return_value=set(range(8))),
                mock.patch.object(supervise.subprocess, "Popen", side_effect=Process),
                mock.patch.object(supervise.time, "sleep"),
            ):
                supervise.run(path)
            self.assertEqual(len(leases), 2)
            self.assertTrue(all(lease.closed for lease in leases))
            self.assertEqual(json.loads((root / "status.json").read_text())["state"], "completed")


if __name__ == "__main__":
    unittest.main()
