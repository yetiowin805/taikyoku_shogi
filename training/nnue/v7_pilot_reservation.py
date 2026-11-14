"""Use the tournament's existing game-boundary demand protocol for pilot CPUs."""

import fcntl
import json
import os
import time
from pathlib import Path


def alive(pid):
    if not isinstance(pid, int) or pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


class Reservation:
    def __init__(self, control, maximum, pid=None):
        if not 1 <= maximum <= 6:
            raise ValueError("pilot reservations must preserve the two game workers")
        self.control = Path(control)
        self.path = self.control / "analysis-demand.json"
        self.maximum = maximum
        self.pid = os.getpid() if pid is None else pid
        self.updated = 0

    def read(self):
        return json.loads(self.path.read_text()) if self.path.exists() else {}

    def write(self, value):
        temporary = self.path.with_name(f".pilot-demand-{self.pid}.tmp")
        temporary.write_text(json.dumps(value) + "\n")
        temporary.replace(self.path)

    def refresh(self):
        now = time.time()
        if now - self.updated >= 5:
            with (self.control / "allocation.lock").open("a+") as lock:
                try:
                    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    return []
                current = self.read()
                if (
                    current.get("pid") != self.pid
                    and alive(current.get("pid"))
                    and now - current.get("updated", 0) < 30
                ):
                    raise RuntimeError(
                        "another live analyzer owns the tournament reservations; pilot did not take them"
                    )
                assigned = current.get("assigned", []) if current.get("pid") == self.pid else []
                if len(assigned) > self.maximum or any(i not in (1, 3, 4, 5, 6, 7) for i in assigned):
                    raise ValueError("invalid pilot CPU reservation")
                self.write(dict(pid=self.pid, pending=self.maximum, assigned=assigned, updated=now, owner="v7-pilot"))
                self.updated = now
        current = self.read()
        return current.get("assigned", []) if current.get("pid") == self.pid else []

    def close(self):
        self.updated = 0
        with (self.control / "allocation.lock").open("a+") as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                return  # The owner's exit/30-second expiry also releases demand.
            if self.read().get("pid") == self.pid:
                self.write(dict(pid=0, pending=0, assigned=[], updated=time.time(), owner="v7-pilot"))
