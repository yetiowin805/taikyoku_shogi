#!/usr/bin/env python3
"""Sequential, CPU-pinned search experiments; no engine build or deployment.

Freeze once before inspecting experimental results. Run baseline and variants in
shuffled order. Raw JSONL/stderr and interrupted iterations are always retained.
All scores emitted by analyze_position are Black-perspective scores.
"""
import argparse
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import platform
import random
import re
import subprocess
import time

HERE = Path(__file__).resolve().parent
MODEL = "models/nnue-v4.5/NNUE_W512_v4.5/model.json"
TARGET_GAME = "data/raw/games/nnue-512-v4.5-handcrafted-losses/slot0673-NNUE_W512_v4.5-vs-SEEDS2-a-white.json"


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()


def capture(command, cwd=None):
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True, check=False)
    return result.stdout.strip() if result.returncode == 0 else None


def source_provenance(repo):
    repo = Path(repo)
    diff = subprocess.run(["git", "diff", "HEAD", "--", "Cargo.toml", "Cargo.lock", "src"],
                          cwd=repo, capture_output=True, check=False)
    # git diff omits untracked source additions; explicitly hash all Rust sources.
    tracked_inputs = sorted(repo.glob("src/**/*.rs")) + [repo / "Cargo.toml", repo / "Cargo.lock"]
    inputs = {str(p.relative_to(repo)): sha(p) for p in tracked_inputs if p.is_file()}
    return {"revision": capture(["git", "rev-parse", "HEAD"], cwd=repo),
            "branch": capture(["git", "branch", "--show-current"], cwd=repo),
            "tracked_source_diff_sha256": hashlib.sha256(diff.stdout).hexdigest(),
            "source_files_sha256": inputs,
            "rustc": capture(["rustc", "--version", "--verbose"]),
            "cargo": capture(["cargo", "--version"])}


def write_json(path, obj):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_suffix(path.suffix + ".tmp")
    temp.write_text(json.dumps(obj, indent=2) + "\n")
    temp.replace(path)


def freeze(args):
    output = Path(args.corpus)
    if output.exists():
        raise SystemExit(f"Refusing to overwrite frozen corpus: {output}")
    root = Path(args.data_root)
    model = json.loads((root / MODEL).read_text())
    blob = (root / MODEL).parent / model["weights"]["nnue"]["file"]
    positions = []

    def add(label, path, ply, depth, group, selection):
        rec = json.loads((root / path).read_text())
        move = rec["moves"][ply - 1]
        positions.append({
            "id": label, "game": path, "game_sha256": sha(root / path),
            "game_id": rec["game_id"], "ply": ply, "fixed_depth": depth,
            "group": group, "selection": selection, "game_plies": len(rec["moves"]),
            "side_to_move": move["color"], "original_move": move,
            "original_agent": rec[move["color"].lower()],
        })

    add("target174", TARGET_GAME, 174, 3, "target", "Previously investigated Tengu sacrifice")
    add("target176", TARGET_GAME, 176, 4, "target", "Previously investigated Crown Prince capture")
    # Preselected without evaluating any search variant. Unrelated means a
    # different game from slot 673, not a statistically independent dataset.
    selections = [
        ("early_v45", "nnue-512-v4.5/slot0636-NNUE_W512_v4.5-vs-NNUE_W1536_v3-a-black.json", 40),
        ("early_mixed", "seeds2-wins-vs-nnue/slot0178-SEEDS2-vs-NNUE_W2048_v3-a-black.json", 61),
        ("middle_v45", "nnue-512-v4.5/slot0632-NNUE_W384_v4.5-vs-NNUE_W512_v4.5-a-black.json", 334),
        ("middle_mixed", "nnue-384-v4.5/slot0644-C2K50A1-vs-NNUE_W384_v4.5-a-black.json", 391),
        ("late_v45", "nnue-512-v4.5/slot0637-NNUE_W512_v4.5-vs-NNUE_W1536_v3-a-white.json", 880),
        ("late_mixed", "nnue-512-v4.5-handcrafted-losses/slot0723-NNUE_W512_v4.5-vs-BASE_C2S2_A160_Lflight-a-white.json", 580),
        ("long_nnue", "long-droughts-under-100/slot0172-NNUE_W512_v3-vs-NNUE_W384_v3-a-black.json", 1301),
        ("long_mixed", "long-droughts-under-100/slot0020-BASE_C2S2_A80_Ldefense-vs-NNUE_W768_v3-a-white.json", 2400),
        ("short_draw", "shortest-nnue-draws/slot0075-NNUE_W512_v3-vs-NNUE_W2048_v3-a-white.json", 35),
        ("long_draw", "shortest-nnue-draws/slot0536-NNUE_W512_v3-vs-NNUE_W2048_v2-a-white.json", 1800),
    ]
    for label, file, ply in selections:
        add(label, "data/raw/games/" + file, ply, 3, "heldout", "Manually frozen stage/length coverage before variant results; no correctness oracle")
    write_json(output, {
        "format_version": 1, "frozen_utc": dt.datetime.now(dt.timezone.utc).isoformat(),
        "model": MODEL, "model_sha256": sha(root / MODEL),
        "model_blob": str(blob.relative_to(root)), "model_blob_sha256": sha(blob),
        "score_perspective": "Black", "ply_convention": "1-based move index; search board BEFORE that move",
        "evaluation_policy": "Fixed W512 v4.5 model on every position; this is a search ablation, not original-agent replay",
        "history_policy": "analyze_position replays the complete original game prefix",
        "heldout_limitation": "Cost and sensitivity controls only; different moves are not automatically improvements or regressions",
        "positions": positions,
    })
    print(f"Frozen {len(positions)} positions at {output}")


def classify(pos, route):
    if not route:
        return "no_move"
    if pos["id"] == "target174":
        if route == "17,31-17,1+":
            return "original_woodland_demon_capture"
        if route.startswith("18,13-"):
            return "tengu_move_candidate_requires_review"
        return "other_requires_review"
    if pos["id"] == "target176":
        if route == "17,1-18,0":
            return "immediate_crown_prince_capture"
        if route == "26,26-26,10+":
            return "original_dragon_king_pawn_capture"
        return "other_requires_review"
    if pos["id"] == "mechanism175":
        return "adaptive_black_reply_comparison"
    return "heldout_no_oracle"


def parse_outputs(stdout, stderr):
    events, parse_errors, pv = [], [], []
    for line in stdout.splitlines():
        if not line.strip():
            continue
        try:
            event = json.loads(line)
            if isinstance(event, dict):
                events.append(event)
        except json.JSONDecodeError:
            parse_errors.append(line[:300])
    for line in stderr.splitlines():
        if line.startswith("PVTRACE "):
            try:
                pv.append(json.loads(line[len("PVTRACE "):]))
            except json.JSONDecodeError:
                pass
    iterations = [e for e in events if e.get("completed_depth") is not None and e.get("event") not in ("complete", "completion", "error", "summary")]
    completions = [e for e in events if e.get("event") in ("complete", "completion", "summary")]
    last = max(iterations, key=lambda e: e.get("completed_depth", 0), default={})
    completion = completions[-1] if completions else {}
    if not last and completion.get("terminal"):
        last = completion
    stats = {}
    for line in stderr.splitlines():
        if line.startswith("STATS "):
            stats = dict(re.findall(r"(\w+)=([^ ]+)", line))
    qnodes = completion.get("q_nodes", completion.get("qnodes", last.get("q_nodes", last.get("qnodes", stats.get("q_nodes")))))
    if qnodes is not None:
        qnodes = int(qnodes)
    return {"events": events, "iterations": iterations, "last_iteration": last,
            "completion": completion, "pv_trace": pv, "parse_errors": parse_errors,
            "q_nodes_final": qnodes, "stderr_stats": stats}


def run(args):
    root = Path(args.data_root).resolve()
    binary = Path(args.binary).resolve()
    corpus = json.loads(Path(args.corpus).read_text())
    variants = json.loads(Path(args.variants_file).read_text())
    requested = set(args.variants.split(",")) if args.variants else set(variants)
    missing = requested - set(variants)
    if missing:
        raise SystemExit(f"Unknown variants: {sorted(missing)}")
    positions = corpus["positions"]
    if args.group != "all":
        positions = [p for p in positions if p["group"] == args.group]
    if args.positions:
        wanted = set(args.positions.split(","))
        positions = [p for p in positions if p["id"] in wanted]
        if wanted != {p["id"] for p in positions}:
            raise SystemExit("Position filter did not resolve all requested IDs")
    if not positions:
        raise SystemExit("No positions selected")
    if not args.model and (sha(root / corpus["model"]) != corpus["model_sha256"] or sha(root / corpus["model_blob"]) != corpus["model_blob_sha256"]):
        raise SystemExit("Model contents differ from frozen corpus")
    model_path = Path(args.model).resolve() if args.model else root / corpus["model"]
    model_contents = json.loads(model_path.read_text())
    nnue = model_contents.get("weights", {}).get("nnue")
    model_provenance = {"file": str(model_path), "sha256": sha(model_path), "override": bool(args.model)}
    if nnue:
        model_blob = model_path.parent / nnue["file"]
        model_provenance.update({"blob": str(model_blob), "blob_sha256": sha(model_blob)})
    for path, checksum in {(p["game"], p["game_sha256"]) for p in positions}:
        if sha(root / path) != checksum:
            raise SystemExit(f"Game contents differ: {path}")
    allowed = sorted(os.sched_getaffinity(0))
    cpu = args.cpu if args.cpu is not None else allowed[0]
    if cpu not in allowed:
        raise SystemExit(f"CPU {cpu} outside permitted affinity {allowed}")
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    manifest = {
        "binary": str(binary), "binary_sha256": sha(binary),
        "corpus_sha256": sha(args.corpus), "data_root": str(root),
        "model_provenance": model_provenance,
        "selected_positions": [p["id"] for p in positions],
        "variants": {k: variants[k] for k in sorted(requested)},
        "mode": args.mode, "fixed_limit_ms": args.fixed_limit_ms,
        "timed_ms": args.timed_ms, "depth_override": args.depth,
        "repetitions": args.repetitions, "shuffle_seed": args.seed, "cpu": cpu,
        "nice": 10, "dump_pv": args.dump_pv, "no_soft_stop": args.no_soft_stop,
        "platform": platform.platform(), "machine": platform.machine(),
        "source_provenance": source_provenance(args.source_root),
        "declared_build_command": args.build_command,
        "declared_rustflags": args.build_rustflags,
        "environment_policy": "Discard inherited TACTICAL_* and TAIKYOKU_AB_*; apply only explicit trial settings",
    }
    manifest_file = out / "manifest.json"
    if manifest_file.exists() and json.loads(manifest_file.read_text()) != manifest:
        raise SystemExit("Output directory belongs to a different run configuration; use a new directory")
    write_json(manifest_file, manifest)
    jobs = [(p, name, repeat) for repeat in range(args.repetitions) for p in positions for name in sorted(requested)]
    random.Random(args.seed).shuffle(jobs)
    for index, (pos, name, repeat) in enumerate(jobs, 1):
        label = f"{pos['id']}-{name}-r{repeat + 1}"
        record_file = out / (label + ".json")
        if record_file.exists():
            print(f"[{index}/{len(jobs)}] retained {label}", flush=True)
            continue
        depth = args.depth or (64 if args.mode == "timed" else pos["fixed_depth"])
        limit = args.timed_ms if args.mode == "timed" else args.fixed_limit_ms
        command = ["nice", "-n", "10", "taskset", "-c", str(cpu), str(binary),
                   str(root / pos["game"]), str(pos["ply"]), str(model_path), str(depth), str(limit)]
        # Do not inherit an unnoticed experimental flag from a parent shell.
        env = {k: v for k, v in os.environ.items() if not k.startswith(("TACTICAL_", "TAIKYOKU_AB_"))}
        env.update(variants[name]["env"])
        if args.no_soft_stop:
            env["TACTICAL_NO_SOFT_STOP"] = "1"
        if args.dump_pv:
            env["TACTICAL_DUMP_PV"] = "1"
        start = time.monotonic()
        external_timeout = False
        with (out / (label + ".jsonl")).open("w") as stdout, (out / (label + ".log")).open("w") as stderr:
            try:
                proc = subprocess.run(command, env=env, stdout=stdout, stderr=stderr,
                                      timeout=max(60, limit / 1000 + 30), check=False)
                returncode = proc.returncode
            except subprocess.TimeoutExpired:
                external_timeout = True
                returncode = None
        elapsed = time.monotonic() - start
        parsed = parse_outputs((out / (label + ".jsonl")).read_text(), (out / (label + ".log")).read_text())
        last = parsed["last_iteration"]
        completed_depth = last.get("completed_depth", 0)
        record = {
            "id": label, "position": pos["id"], "variant": name, "repetition": repeat + 1,
            "group": pos["group"], "mode": args.mode, "requested_depth": depth,
            "search_limit_ms": limit, "command": command,
            "env": {k: v for k, v in env.items() if k.startswith("TACTICAL_")},
            "returncode": returncode, "external_timeout": external_timeout,
            "wall_seconds": elapsed, "completed_depth": completed_depth,
            "fixed_depth_complete": args.mode == "fixed" and (completed_depth >= depth or last.get("terminal", False)),
            "best_move": last.get("best_move"), "score_black": last.get("score"),
            "nodes_last_iteration": last.get("nodes"), "search_elapsed_ms": last.get("elapsed_ms"),
            "move_category": classify(pos, last.get("best_move")), **parsed,
        }
        write_json(record_file, record)
        print(f"[{index}/{len(jobs)}] {label}: d{completed_depth} {record['best_move']} black={record['score_black']} {elapsed:.2f}s complete={record['fixed_depth_complete']}", flush=True)
    print(f"Results retained under {out}", flush=True)


def summarize(args):
    records = []
    for directory in args.runs:
        for path in sorted(Path(directory).glob("*.json")):
            if path.name == "manifest.json":
                continue
            record = json.loads(path.read_text())
            if "position" in record and "variant" in record:
                record["source_file"] = str(path)
                records.append(record)
    # Keep complete iteration evidence, never count a time-limited partial depth
    # as an equal-depth observation.
    write_json(args.output, {"format_version": 1, "runs": records})
    print(f"Collected {len(records)} trials into {args.output}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="action", required=True)
    freeze_p = sub.add_parser("freeze")
    freeze_p.add_argument("--data-root", required=True)
    freeze_p.add_argument("--corpus", default=str(HERE / "corpus.json"))
    freeze_p.set_defaults(func=freeze)
    run_p = sub.add_parser("run")
    run_p.add_argument("--binary", required=True)
    run_p.add_argument("--source-root", default=str(HERE.parents[2]))
    run_p.add_argument("--build-command", default="See build record; not supplied")
    run_p.add_argument("--build-rustflags", default="See build record; not supplied")
    run_p.add_argument("--data-root", required=True)
    run_p.add_argument("--model", help="Explicit alternate teacher checkpoint; hash recorded separately from frozen model")
    run_p.add_argument("--corpus", default=str(HERE / "corpus.json"))
    run_p.add_argument("--variants-file", default=str(HERE / "variants.json"))
    run_p.add_argument("--variants", help="comma-separated names; default all")
    run_p.add_argument("--group", choices=("target", "heldout", "all"), default="target")
    run_p.add_argument("--positions", help="comma-separated corpus IDs")
    run_p.add_argument("--mode", choices=("fixed", "timed"), default="fixed")
    run_p.add_argument("--depth", type=int)
    run_p.add_argument("--fixed-limit-ms", type=int, default=20000)
    run_p.add_argument("--timed-ms", type=int, default=3000)
    run_p.add_argument("--repetitions", type=int, default=1)
    run_p.add_argument("--seed", type=int, default=20261008)
    run_p.add_argument("--cpu", type=int)
    run_p.add_argument("--dump-pv", action="store_true", help="Diagnostic TT PV traces; leave disabled for timing")
    run_p.add_argument("--no-soft-stop", action="store_true", help="Disable predictive ID stopping; retain requested search deadline")
    run_p.add_argument("--output", required=True)
    run_p.set_defaults(func=run)
    summary_p = sub.add_parser("summarize")
    summary_p.add_argument("runs", nargs="+")
    summary_p.add_argument("--output", required=True)
    summary_p.set_defaults(func=summarize)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
