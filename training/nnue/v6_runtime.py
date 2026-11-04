"""Concurrent engine requests with all catalogue mutations confined to the supervisor."""

import os
import queue
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import v6_data as data
import v6_teachers as teachers


class Worker:
    def __init__(self, index, lease, config, db, steps, oracle_factory):
        self.index = index
        self.cpu = config["cpus"][index]
        self.lease = lease
        self.config = config
        self.db = db
        self.make_steps = steps
        self.oracle_factory = oracle_factory
        self.executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix=f"v6-cpu-{self.cpu}")
        self.events = queue.Queue()
        self.oracle = None
        self.binding = None
        self.future = None
        self.job = None
        self.steps = None
        self.receive = None
        self.context = None

    @property
    def idle(self):
        return self.job is None

    @property
    def ready(self):
        return self.oracle.ready

    @property
    def active(self):
        if self.idle:
            return None
        position, game, config = self.context
        return dict(
            cpu=self.cpu,
            job=self.job["id"],
            tier=self.job["tier"],
            teacher=self.job["teacher"],
            teacher_id=config["models"][self.job["teacher"]].get("id"),
            teacher_epoch=config.get("teacher_epoch"),
            game=game["id"],
            ply=position["ply"],
        )

    def request(self, request):
        # Linux affinity is per-thread: the child inherits this worker's one leased CPU.
        os.sched_setaffinity(0, {self.cpu})
        _, game, config = self.context
        teacher = config["models"][self.job["teacher"]]
        key = (game["id"], self.job["teacher"], teacher.get("sha256"))
        if key != self.binding:
            self.reset_oracle()
            if data.file_hash(game["path"]) != game["id"]:
                raise ValueError("frozen game changed")
            teachers.validate(teacher)
            environment = dict(os.environ, TACTICAL_LEASE_FD=str(self.lease.fileno()))
            self.oracle = self.oracle_factory(
                dict(config, environment=environment),
                dict(game=game["path"], start_ply=1),
                Path(config["out"]),
                teacher["path"],
            )
            if self.oracle.ready.get("collection_protocol") != 1:
                raise ValueError("worker lacks v6 collection protocol")
            self.binding = key
        return self.oracle.call(request, self.events.put)

    def assign(self, job, position, game, config):
        if not self.idle:
            raise RuntimeError("cannot assign a busy collection worker")
        self.job = dict(job)
        self.context = (position, game, config)
        self.steps = self.make_steps(self.db, config, self.job, position, self)
        self.advance(None)

    def advance(self, response):
        try:
            request, self.receive = self.steps.send(response)
            # No open transaction crosses an engine request or another job's callbacks.
            self.db.commit()
            self.future = self.executor.submit(self.request, request)
        except StopIteration as done:
            self.db.execute(
                "UPDATE jobs SET status='completed',result=?,error=NULL WHERE id=?",
                (data.encoded(done.value), self.job["id"]),
            )
            self.db.commit()
            self.job = None
            self.steps = None
            self.future = None
        except Exception as error:
            self.fail(error)

    def drain(self):
        while True:
            try:
                event = self.events.get_nowait()
            except queue.Empty:
                return
            self.receive(event)

    def poll(self):
        if self.future is None:
            return
        self.drain()
        if not self.future.done():
            return
        future = self.future
        self.future = None
        try:
            response = future.result()
        except Exception as error:
            self.drain()
            self.fail(error)
            return
        # Catch events queued between the first drain and the future becoming ready.
        self.drain()
        self.advance(response)

    def fail(self, error):
        self.db.rollback()
        if isinstance(error, InterruptedError):
            self.db.execute("UPDATE jobs SET status='pending',attempts=max(0,attempts-1) WHERE id=?", (self.job["id"],))
        else:
            state = "failed" if self.job["attempts"] + 1 >= self.config["max_attempts"] else "pending"
            self.db.execute("UPDATE jobs SET status=?,error=? WHERE id=?", (state, str(error), self.job["id"]))
            print(f"job {self.job['id']} {state}: {error}", file=sys.stderr, flush=True)
        self.db.commit()
        self.steps.close()
        self.steps = None
        self.job = None
        self.future = None
        self.reset_oracle()

    def reset_oracle(self):
        if self.oracle:
            self.oracle.close()
            self.oracle = None
        self.binding = None

    def close(self):
        try:
            self.executor.shutdown(wait=True)
            # The supervisor consumes any final iteration events before resetting interrupted work.
            self.drain()
            if self.steps:
                self.steps.close()
            self.reset_oracle()
        finally:
            self.lease.close()
