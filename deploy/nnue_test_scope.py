"""Select expensive NNUE process checks from the actual CI checkout diff.

Every run keeps the fast suite. Manual runs and unknown diffs run integrations;
documentation, model weights, and Rust-only changes do not need Python trainer
process checks. Deployment Python is deliberately broad because the trainer and
continuation tests import shared storage and supervisor helpers from there.
"""

import json
import os
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def needs_integration(paths):
    return any(
        (path.startswith(("training/nnue/", "deploy/")) and path.endswith(".py"))
        or path.startswith(".github/")
        or path in ("training/nnue/requirements.txt", "pyproject.toml", ".python-version")
        for path in paths
    )


def changed_paths(base, *, repo=ROOT):
    if not re.fullmatch(r"[0-9a-fA-F]{40}", base) or set(base) == {"0"}:
        raise ValueError("No usable base commit")
    # Include both sides of renames, including code renamed out of a tested path.
    raw = subprocess.check_output(["git", "diff", "--name-only", "--no-renames", "-z", base, "HEAD", "--"], cwd=repo)
    return raw.decode().rstrip("\0").split("\0") if raw else []


def scope(event_name, event, *, diff=changed_paths):
    if event_name == "workflow_dispatch":
        return True, "manual full run"
    if event_name == "pull_request":
        base = event.get("pull_request", {}).get("base", {}).get("sha", "")
    elif event_name == "push":
        base = event.get("before", "")
    else:
        return True, "unknown event; run full suite"
    try:
        paths = diff(base)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        return True, f"diff unavailable; run full suite: {error}"
    integration = needs_integration(paths)
    return integration, "trainer/deployment/CI inputs changed" if integration else "no Python trainer inputs changed"


def main():
    try:
        event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text())
        integration, reason = scope(os.environ.get("GITHUB_EVENT_NAME", ""), event)
    except (KeyError, OSError, ValueError) as error:
        integration, reason = True, f"event unavailable; run full suite: {error}"
    print(json.dumps(dict(integration=integration, reason=reason)))
    if output := os.environ.get("GITHUB_OUTPUT"):
        with Path(output).open("a") as target:
            target.write(f"integration={str(integration).lower()}\n")


if __name__ == "__main__":
    main()
