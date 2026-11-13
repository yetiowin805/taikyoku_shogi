"""Attach generation analysis updates to the existing local dashboard refresher."""

import argparse
import re
from pathlib import Path


def install(web, work, python):
    target = web / "finish_generation_dashboard.py"
    source = target.read_text()
    start = "    # BEGIN GENERATION ANALYSIS REFRESH\n"
    end = "    # END GENERATION ANALYSIS REFRESH\n"
    source = re.sub(re.escape(start) + r".*?" + re.escape(end), "", source, flags=re.S)
    anchor = '    print("Published", payload["deployment"]["stage"], state["_checked_utc"], flush=True)'
    if source.count(anchor) != 1:
        raise ValueError("Cannot identify the dashboard publication hook")
    refresh = Path(__file__).resolve().parents[1] / "generation_strength_refresh.py"
    block = (
        start
        + f"    analysis_work = Path({str(work)!r})\n"
        + "    analysis_work.mkdir(parents=True, exist_ok=True)\n"
        + "    analysis_snapshot = analysis_work / 'dashboard-input.json'\n"
        + "    analysis_snapshot.write_text(json.dumps({'state': state, 'fit': fit}))\n"
        + f"    subprocess.run([{str(python)!r}, {str(refresh)!r}, 'request', '--snapshot', str(analysis_snapshot), '--work-dir', str(analysis_work), '--web-dir', str(OUT)], check=True)\n"
        + end
    )
    backup = target.with_suffix(".py.before-generation-auto-update")
    if not backup.exists():
        backup.write_text(target.read_text())
    target.write_text(source.replace(anchor, block + anchor))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--web-dir", type=Path, required=True)
    parser.add_argument("--work-dir", type=Path, required=True)
    parser.add_argument("--python", type=Path, required=True)
    args = parser.parse_args()
    install(args.web_dir.resolve(), args.work_dir.resolve(), args.python.absolute())


if __name__ == "__main__":
    main()
