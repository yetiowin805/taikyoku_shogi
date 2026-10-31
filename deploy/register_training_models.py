"""Register newly admitted immutable model bindings for future teacher promotion.

Called by the training service's ExecStopPost, including recovery. Existing
bindings are never changed. No new teacher is selected and no positions reanalyzed.
"""

import argparse
import subprocess
from pathlib import Path

import tourney_analysis as a


def merge_bindings(old, current):
    result = {}
    for models in (old, current):
        for path, binding in models.items():
            # Keep the original key for saved requests and add the canonical
            # spelling used by winner selection and position search.
            for alias in (path, str(Path(path).resolve())):
                if alias in result and result[alias] != binding:
                    raise ValueError("Existing teacher binding changed: " + alias)
                result[alias] = binding
    return result


def main():
    p = argparse.ArgumentParser()
    p.add_argument("run", type=Path)
    args = p.parse_args()
    control = args.run / "analysis"
    dual = control / "dual-label-config.json"
    if not dual.exists():
        return
    current = a.read(control / "config.json")["models"]
    before = a.read(dual)
    if merge_bindings(before["old"]["models"], current) == before["old"]["models"]:
        return
    # The analysis manager owns this config while running. Let its normal stop
    # persist completed searches, then update without racing a teacher switch.
    subprocess.run(["systemctl", "stop", "taikyoku-dual-labels.service"], check=True)
    try:
        config = a.read(dual)
        current = a.read(control / "config.json")["models"]
        for path, binding in current.items():
            if path not in config["old"]["models"]:
                a.validate_model_binding(path, binding)
        config["old"]["models"] = merge_bindings(config["old"]["models"], current)
        a.atomic(dual, config)
    finally:
        subprocess.run(["systemctl", "start", "taikyoku-dual-labels.service"], check=True)


if __name__ == "__main__":
    main()
