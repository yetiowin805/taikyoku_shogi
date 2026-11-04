"""Collection provenance, split isolation, bounded selection and resume contracts."""

import json
import signal
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

import v6_collect as collect
import v6_data as data


def game(count=800):
    return dict(
        result="BlackWins",
        moves=[
            dict(color="Black" if i % 2 == 0 else "White", eval=(i % 10) * 100, static_eval=0) for i in range(count)
        ],
    )


class CollectionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.db = data.connect(self.root)
        self.addCleanup(self.db.close)
        self.config = dict(
            collect.defaults(), out=str(self.root), primary="p", challenger="c", cpus=[0, 1, 2, 3], lease_index=3
        )

    def position(self, key="g", split="train", ply=10):
        row = dict(
            game_id=key,
            ply=ply,
            group=key,
            split=split,
            inherited=False,
            stratum="representative",
            path=[],
            parent=None,
        )
        row["id"] = data.add_position(self.db, row)
        return row

    def test_sampler_is_bounded_spaced_and_keeps_random_holdouts(self):
        source = game()
        first = data.select_roots(source, "game", "train", 42)
        self.assertEqual(first, data.select_roots(source, "game", "train", 42))
        ordinary = [row for row in first if row["stratum"] == "representative"]
        challenge = [row for row in first if row["stratum"] == "challenge"]
        self.assertEqual(len(ordinary), 32)
        self.assertLessEqual(len(challenge), 8)
        self.assertEqual({row["color"] for row in ordinary}, {"Black", "White"})
        self.assertTrue(all(abs(a["ply"] - b["ply"]) >= 8 for i, a in enumerate(ordinary) for b in ordinary[i + 1 :]))
        self.assertGreaterEqual(sum(row["selection"] == "random_challenge" for row in challenge), 2)
        held = data.select_roots(source, "game", "test", 42)
        source["result"] = "WhiteWins"
        for move in source["moves"]:
            move["eval"] *= -1
        held_after = data.select_roots(source, "game", "test", 42)
        self.assertEqual([(r["ply"], r["selection"]) for r in held], [(r["ply"], r["selection"]) for r in held_after])

    def test_related_games_inherit_conflicting_parent_exposure_as_quarantine(self):
        records = {key: dict(pair="pair", prefix=key, start_kind="opening", start_hash="opening") for key in ("a", "b")}
        data.assign_groups(records, {"a": {("p1", "train")}, "b": {("p2", "test")}}, 42)
        self.assertEqual(records["a"]["group"], records["b"]["group"])
        self.assertTrue(all(row["quarantine"] for row in records.values()))

    def test_standard_opening_alone_does_not_merge_games(self):
        records = {key: dict(pair=key, prefix=key, start_kind="opening", start_hash="opening") for key in ("a", "b")}
        data.assign_groups(records, {}, 42)
        self.assertNotEqual(records["a"]["group"], records["b"]["group"])

    def test_partial_root_and_candidate_bounds_never_become_labels(self):
        event = dict(event="iteration", depth=2, score_black=-120, best={"move": "full-route"}, nodes=99, candidates=[])
        final = dict(completed_depth=2, partial_root_depth=3, score_black=99999, best={"bad": "partial"})
        label = collect.accepted_label([event], final)
        self.assertEqual(label["score_black"], -120)
        self.assertEqual(label["completed_depth"], 2)
        self.assertFalse(label["mate_verified"])
        with self.assertRaises(ValueError):
            collect.accepted_label([], final)
        self.assertIsNone(collect.accepted_label([], dict(terminal=0)))

    def test_parent_and_cross_split_duplicates_revoke_holdout_eligibility(self):
        class Oracle:
            def call(self, request, receive):
                return dict(canonical_position_hash="features", terminal=None)

        first = self.position("a", "validation")
        second = self.position("b", "train")
        job = dict(teacher="p")
        collect.cache_probe(self.db, self.config, job, first, Oracle())
        self.assertEqual(
            self.db.execute("SELECT holdout_eligible FROM positions WHERE id=?", (first["id"],)).fetchone()[0], 1
        )
        collect.cache_probe(self.db, self.config, job, second, Oracle())
        self.assertEqual(self.db.execute("SELECT sum(holdout_eligible) FROM positions").fetchone()[0], 0)
        self.db.execute("INSERT INTO parent_features VALUES('features')")
        third = self.position("c", "test")
        collect.cache_probe(self.db, self.config, job, third, Oracle())
        self.assertEqual(self.db.execute("SELECT sum(holdout_eligible) FROM positions").fetchone()[0], 0)

    def test_job_and_branch_planning_are_idempotent_and_do_not_inherit_outcome(self):
        position = self.position()
        for _ in range(2):
            data.add_job(self.db, position["id"], "p", "alternatives", 0, 30)
        moves = [{"from": {"file": i, "rank": 1}, "to": {"file": i, "rank": 2}} for i in range(3)]
        for _ in range(2):
            collect.plan_children(self.db, self.config, position, moves)
        children = [
            json.loads(row[0])
            for row in self.db.execute("SELECT payload FROM positions")
            if json.loads(row[0]).get("parent")
        ]
        self.assertEqual(len(children), 3)
        self.assertTrue(all(row["outcome"] is None and not row["outcome_eligible"] for row in children))
        self.assertTrue(all(row["group"] == position["group"] and row["split"] == "train" for row in children))
        self.assertEqual(self.db.execute("SELECT count(*) FROM jobs WHERE tier='child_audit'").fetchone()[0], 4)
        self.assertEqual(self.db.execute("SELECT count(*) FROM jobs WHERE tier='child_deep'").fetchone()[0], 4)

    def test_failed_pilot_cannot_be_bypassed_by_restart(self):
        data.set_meta(self.db, "pilot_summary", dict(passed=False))
        with self.assertRaisesRegex(ValueError, "pilot evidence gate failed"):
            collect.pilot_summary(self.db, self.config)

    def test_alternative_move_identity_preserves_route_data(self):
        first = dict(from_file=1, to_file=2, data={"via": 3})
        second = dict(from_file=1, to_file=2, data={"via": 4})
        selected = collect.choose_alternatives(
            first, [dict(move=first, static_root=1), dict(move=second, static_root=2)]
        )
        self.assertEqual(selected, [first, second])

    def test_terminal_outcome_is_verified_independently_of_saved_winner(self):
        position = self.position()
        position["outcome"] = "WhiteWins"
        self.db.execute("INSERT INTO games VALUES(?,?)", ("g", data.encoded(dict(id="g", result="WhiteWins"))))

        def probe(value):
            yield from ()
            return dict(terminal=value)

        with patch.object(collect, "probe_steps", return_value=probe(1)):
            result = collect.execute(self.db, self.config, dict(tier="outcome"), position, None)
        self.assertFalse(result["outcome_eligible"])
        with patch.object(collect, "probe_steps", return_value=probe(-1)):
            result = collect.execute(self.db, self.config, dict(tier="outcome"), position, None)
        self.assertTrue(result["outcome_eligible"])

    def test_complete_queue_and_restart_keep_committed_work(self):
        self.exercise_queue(1)

    def test_six_workers_overlap_without_duplicate_jobs_and_resume_cleanly(self):
        self.exercise_queue(6)

    def test_explicit_stop_retains_iteration_evidence_and_requeues_interrupted_work(self):
        self.exercise_queue(2, stop_early=True)

    def exercise_queue(self, workers, stop_early=False):
        source = self.root / "source.json"
        source.write_text(data.encoded(dict(game(80), format_version=2, start=dict(kind="opening"))))
        state = dict(run_id="test", slots=[dict(id=1, status="done", start_seed=9, game_path=str(source))])
        state_path = self.root / "source-state.json"
        state_path.write_text(data.encoded(state))
        parent = self.root / "parent.jsonl"
        parent.write_text(data.encoded(dict(split="train", canonical_position_hash="old-feature")) + "\n")
        binary = self.root / "worker"
        binary.write_text("fixture")
        config = dict(
            self.config,
            source_repo=str(self.root),
            control=str(self.root),
            binary=str(binary),
            state_sha256=data.file_hash(state_path),
            parent_samples=[str(parent)],
            identities={str(binary): data.file_hash(binary)},
            models={"p": dict(path="p"), "c": dict(path="c")},
            diagnostic=None,
            workers=workers,
            cpus=list(range(8)),
            lease_indices=[7, 1, 3, 4, 5, 6][:workers],
            minimum_free_bytes=0,
            representative=4,
            challenge=2,
            pilot_roots=1,
            pilot_challenger=1,
            audit_roots=2,
            deep_roots=1,
            branch_roots=1,
            branch_audit_roots=1,
            branch_deep_roots=1,
        )
        # Choose a training split for this finite fixture; ordinary production uses its frozen seed.
        for seed in range(100):
            config["seed"] = seed
            if int(data.hashed([seed, data.file_hash(source)])[:8], 16) % 10 >= 2:
                break
        config_path = self.root / "config.json"
        config_path.write_text(data.encoded(config))
        calls = []
        lock = threading.Lock()
        activity = dict(active=0, maximum=0)
        lease_fds = set()
        timers = []

        class Oracle:
            ready = dict(collection_protocol=1, search_config="teacher")

            def __init__(self, config, *args):
                with lock:
                    lease_fds.add(config["environment"]["TACTICAL_LEASE_FD"])

            def close(self):
                pass

            def call(self, request, receive):
                calls.append(request)
                if request["op"] == "collection_probe":
                    return dict(
                        canonical_position_hash=data.hashed([request["root_ply"], request["path"]]),
                        terminal=0 if request["root_ply"] == 81 else None,
                        static_black=10,
                        quiescence=dict(aborted=False, score_black=15),
                    )
                move = dict(from_file=1, to_file=2)
                if request["op"] == "rank_candidates":
                    return dict(
                        candidates=[
                            dict(move=move, static_root=3),
                            dict(move=dict(from_file=2, to_file=3), static_root=4),
                        ]
                    )
                event = dict(
                    event="iteration",
                    depth=2,
                    score_black=20,
                    best=move,
                    nodes=100,
                    candidates=[dict(move=move, score_black=20)],
                )
                with lock:
                    activity["active"] += 1
                    activity["maximum"] = max(activity["maximum"], activity["active"])
                try:
                    receive(event)
                    if stop_early:
                        timer = threading.Timer(0.3, collect.STOP.set)
                        with lock:
                            timers.append(timer)
                        timer.start()
                    if collect.STOP.wait(5 if stop_early else 0.08):
                        raise InterruptedError("fixture interrupted")
                finally:
                    with lock:
                        activity["active"] -= 1
                return dict(event="complete", completed_depth=2, aborted=False)

        prior_handlers = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
        try:
            with patch.object(collect, "Oracle", Oracle), patch.object(collect.os, "sched_setaffinity"):
                collect.run(config_path)
                before = len(calls)
                if not stop_early:
                    collect.run(config_path)
            self.assertEqual(len(calls), before)
            status = json.loads((self.root / "status.json").read_text())
            if stop_early:
                self.assertEqual(status["state"], "stopped")
                self.assertEqual(
                    self.db.execute("SELECT count(*) FROM jobs WHERE status IN ('running','failed')").fetchone()[0], 0
                )
                self.assertGreater(self.db.execute("SELECT count(*) FROM iterations").fetchone()[0], 0)
                self.assertGreater(status["jobs"]["pending"], 0)
                return
            self.assertEqual(status["state"], "completed")
            if workers > 1:
                self.assertGreater(activity["maximum"], 1)
                self.assertGreater(len(lease_fds), 1)
            self.assertLessEqual(activity["maximum"], workers)
            self.assertEqual(self.db.execute("SELECT max(attempts) FROM jobs").fetchone()[0], 1)
            self.assertGreater(status["jobs"]["completed"], 15)
            self.assertEqual(self.db.execute("SELECT count(*) FROM jobs WHERE status!='completed'").fetchone()[0], 0)
            self.assertTrue(data.get_meta(self.db, "pilot_summary")["passed"])
            searches = [request for request in calls if request["op"] == "search"]
            self.assertTrue(searches)
            self.assertTrue(all("quiescence_depth" not in r and "collection_policy" not in r for r in searches))
        finally:
            # A request dispatched during shutdown can schedule a second test signal.
            # Never let that delayed fixture signal interrupt the following test/run.
            for timer in timers:
                timer.cancel()
                timer.join()
            for sig, handler in prior_handlers.items():
                signal.signal(sig, handler)

    def test_resize_preserves_evidence_and_backs_up_original_catalogue(self):
        config = dict(self.config, cpus=list(range(8)), parent_samples=["fixture"], identities={}, minimum_free_bytes=0)
        config_path = self.root / "config.json"
        config_path.write_text(data.encoded(config))
        old_hash = data.file_hash(config_path)
        position = self.position()
        data.add_job(self.db, position["id"], "p", "broad", 10, 0)
        self.db.execute(
            "UPDATE jobs SET status='completed',result=?,attempts=1", (data.encoded(dict(evidence="preserve")),)
        )
        data.set_meta(self.db, "initialized", dict(games=1))
        data.set_meta(self.db, "config_sha256", old_hash)
        data.set_meta(self.db, "teacher_root:" + position["id"], "immutable-binding")
        self.db.commit()
        args = SimpleNamespace(
            config=config_path,
            lease_indices=[7, 1, 3, 4, 5, 6],
            revision="test",
            audit_roots=5000,
            deep_roots=1000,
            branch_roots=1000,
            branch_audit_roots=500,
            branch_deep_roots=100,
        )
        collect.resize(args)
        new_path = self.root / "config-resized.json"
        new = json.loads(new_path.read_text())
        self.assertEqual(data.file_hash(config_path), old_hash)
        self.assertEqual(new["workers"], 6)
        self.assertNotIn("stop_at", new)
        self.assertEqual(data.get_meta(self.db, "config_sha256"), data.file_hash(new_path))
        self.assertEqual(data.get_meta(self.db, "teacher_root:" + position["id"]), "immutable-binding")
        self.assertEqual(
            json.loads(self.db.execute("SELECT result FROM jobs").fetchone()[0]), dict(evidence="preserve")
        )
        self.assertTrue((self.root / "before-resize.sqlite").is_file())
        for indices in ([0], [2], [1, 1], [1, 3, 4, 5, 6, 7, 8]):
            with self.assertRaises(ValueError):
                data.validate_config(dict(new, workers=len(indices), lease_indices=indices))

    def teacher_fixture(self):
        run = self.root / "tournament"
        run.joinpath("analysis").mkdir(parents=True)
        entrants = []
        registry = {}
        for name, network in (("leader", "net1"), ("same-net", "net1"), ("other-net", "net2"), ("handcrafted", None)):
            model = run / (name + ".json")
            weights = {}
            if network:
                blob = run / (network + ".nnue")
                blob.write_text(network)
                weights["nnue"] = dict(file=blob.name, sha256=data.file_hash(blob))
            model.write_text(data.encoded(dict(name=name, weights=weights, search_defaults=dict(trial=name))))
            entrants.append(dict(id=name, model=str(model)))
            registry[str(model)] = dict(sha256=data.file_hash(model))
        (run / "state.json").write_text(data.encoded(dict(entrants=entrants, retired=[], slots=[])))
        (run / "order-neutral-ratings.json").write_text(
            data.encoded(
                dict(
                    status="converged",
                    games=0,
                    ratings={"leader": 1700, "same-net": 1690, "other-net": 1650, "handcrafted": 1400},
                )
            )
        )
        (run / "analysis/config.json").write_text(data.encoded(dict(models=registry)))
        config = dict(
            self.config, teacher_policy=collect.teachers.POLICY, teacher_run=str(run), teacher_refresh_seconds=60
        )
        config["initial_teachers"] = collect.teachers.snapshot(config)
        return config, run

    def test_standings_select_search_agent_and_distinct_network_not_parent(self):
        config, run = self.teacher_fixture()
        models = config["initial_teachers"]["models"]
        self.assertEqual(models["primary"]["id"], "leader")
        self.assertEqual(models["challenger"]["id"], "other-net")
        self.assertEqual(models["diagnostic"]["id"], "handcrafted")
        self.assertEqual(
            json.loads(Path(models["primary"]["path"]).read_text())["search_defaults"], dict(trial="leader")
        )
        fit_path = run / "order-neutral-ratings.json"
        fit = json.loads(fit_path.read_text())
        fit["ratings"]["same-net"] = 1700
        fit_path.write_text(data.encoded(fit))
        current = collect.teachers.snapshot(config, config["initial_teachers"])
        self.assertEqual(current["models"]["primary"]["id"], "leader")
        state_path = run / "state.json"
        state = json.loads(state_path.read_text())
        state["retired"] = ["leader"]
        state_path.write_text(data.encoded(state))
        self.assertEqual(collect.teachers.snapshot(config, current)["models"]["primary"]["id"], "same-net")

    def test_champion_changes_only_rebind_new_root_families(self):
        config, run = self.teacher_fixture()
        first = self.position(ply=10)
        bound = collect.teachers.bind(self.db, config, first)
        fit_path = run / "order-neutral-ratings.json"
        fit = json.loads(fit_path.read_text())
        fit["ratings"]["other-net"] = 1800
        fit_path.write_text(data.encoded(fit))
        data.set_meta(self.db, "teacher_refresh_after", 0)
        second = collect.teachers.bind(self.db, config, self.position(ply=20))
        self.assertEqual(second["models"]["primary"]["id"], "other-net")
        self.assertEqual(collect.teachers.bind(self.db, config, first)["teacher_epoch"], bound["teacher_epoch"])
        child = dict(first, id="child", parent=first["id"], path=[{}])
        self.assertEqual(collect.teachers.bind(self.db, config, child)["teacher_epoch"], bound["teacher_epoch"])
        # A stale/error fit retains the last verified teachers for future work and reports it.
        fit["games"] = 100
        fit_path.write_text(data.encoded(fit))
        data.set_meta(self.db, "teacher_refresh_after", 0)
        third = collect.teachers.bind(self.db, config, self.position(ply=30))
        self.assertEqual(third["teacher_epoch"], second["teacher_epoch"])
        self.assertEqual(json.loads((self.root / "teacher-status.json").read_text())["state"], "retained_after_error")
        frozen = Path(bound["models"]["primary"]["path"])
        frozen.write_text("tampered")
        with self.assertRaisesRegex(ValueError, "frozen teacher changed"):
            collect.teachers.validate(bound["models"]["primary"])


if __name__ == "__main__":
    unittest.main()
