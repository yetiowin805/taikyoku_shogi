#!/usr/bin/env python3
"""Prepare native or verified host-local PGO bundles, then publish between games."""

import argparse
import contextlib
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import engine_updates
import pgo_build
import tourney_analysis as analysis

ROOT = Path(__file__).resolve().parent.parent


def interrupted(signum, _frame):
    # selectors retries InterruptedError, including while communicate() reads
    # a captured tool response. Escape into process-group cleanup instead.
    raise RuntimeError(f"engine update interrupted by signal {signum}")


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("run", type=Path)
    result.add_argument("--compatible-speedup", action="store_true", required=True)
    sources = result.add_mutually_exclusive_group()
    sources.add_argument("ref", nargs="?", help="Git ref, default origin/main; fetches origin")
    sources.add_argument(
        "--source-dir", type=Path, help="exact source archive/checkout for PGO; never substitutes main"
    )
    result.add_argument("--pgo", action="store_true")
    result.add_argument("--out", type=Path, help="new durable PGO directory; defaults beneath RUN/analysis/pgo-builds")
    result.add_argument("--llvm-profdata", default="auto")
    result.add_argument("--cpu", type=int, default=min(os.sched_getaffinity(0)))
    result.add_argument("--models", nargs="+", help="active entrant IDs for profiling and verification")
    result.add_argument("--max-models", type=int, default=6)
    result.add_argument("--games", type=int, default=3)
    result.add_argument("--verify-games", type=int, default=2)
    result.add_argument("--roots-per-game", type=int, default=2)
    result.add_argument("--recent-games", type=int, default=30)
    result.add_argument("--profile-repeats", type=int, default=2)
    result.add_argument("--verify-repeats", type=int, default=4)
    result.add_argument("--depth", type=int, default=2)
    result.add_argument("--nice", type=int, default=19)
    result.add_argument("--prepare-only", action="store_true", help="PGO: retain verified outputs without publishing")
    return result


def validate_args(args):
    if not args.pgo and (args.source_dir or args.out or args.prepare_only):
        raise ValueError("--source-dir, --out and --prepare-only require --pgo")
    for name in (
        "max_models",
        "games",
        "verify_games",
        "roots_per_game",
        "recent_games",
        "profile_repeats",
        "verify_repeats",
        "depth",
    ):
        if getattr(args, name) < 1:
            raise ValueError("--" + name.replace("_", "-") + " must be positive")
    if args.verify_repeats % 2:
        raise ValueError("--verify-repeats must be even to balance paired arm order")
    if args.cpu not in os.sched_getaffinity(0):
        raise ValueError("--cpu must be inside the allowed affinity mask")
    if not 0 <= args.nice <= 19:
        raise ValueError("--nice must be between 0 and 19")


def stop_group(proc):
    # A stage can own a nested build helper. Termination must not leave a
    # compiler or search behind after the immediate helper exits.
    with contextlib.suppress(ProcessLookupError):
        os.killpg(proc.pid, signal.SIGTERM)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        pass
    with contextlib.suppress(ProcessLookupError):
        os.killpg(proc.pid, signal.SIGKILL)
    proc.wait()


class Update:
    def __init__(self, args, output=None):
        self.args = args
        self.output = output
        self.stage = "preflight"
        self.publication_attempted = False
        self.metadata = dict(status="running", started=time.time(), commands=[], run=str(args.run))

    def save(self, **values):
        self.metadata.update(values, stage=self.stage, updated=time.time())
        if self.output is not None:
            analysis.atomic(self.output / "update.json", self.metadata)

    def command(self, stage, argv, *, cwd=None, env=None, capture=False, nice=False):
        self.stage = stage
        argv = list(map(str, argv))
        if nice:
            argv = ["nice", "-n", str(self.args.nice), *argv]
        record = dict(stage=stage, argv=argv, cwd=str(cwd or ROOT), started=time.time())
        if env is not None:
            record["environment"] = {
                key: env[key]
                for key in ("CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS", "RUSTC")
                if key in env
            }
        self.metadata["commands"].append(record)
        log = self.output / f"{len(self.metadata['commands']):02d}-{stage}.log" if self.output else None
        if log is not None:
            record["log"] = str(log)
        self.save()
        print(f"Engine update: {stage}" + (f"; log={log}" if log else ""), flush=True)
        with contextlib.ExitStack() as stack:
            stdout = stack.enter_context(log.open("wb")) if log else None
            proc = subprocess.Popen(
                argv,
                cwd=cwd or ROOT,
                env=env,
                stdout=subprocess.PIPE if capture else stdout,
                stderr=stdout if log else None,
                start_new_session=True,
                text=capture,
            )
            try:
                response, _ = proc.communicate()
            except BaseException:
                stop_group(proc)
                raise
            finally:
                record.update(returncode=proc.returncode, finished=time.time())
                self.save()
            if capture:
                record["stdout"] = response
                if log is not None:
                    stdout.write(response.encode())
                self.save()
            if proc.returncode:
                raise RuntimeError(f"command exited {proc.returncode}" + (f"; see {log}" if log else ""))
            return response if capture else None


@contextlib.contextmanager
def source_tree(args, update):
    if args.source_dir:
        source = args.source_dir.resolve(strict=True)
        if not source.is_dir() or not (source / "Cargo.toml").is_file():
            raise ValueError("--source-dir must contain the exact source tree and Cargo.toml")
        yield source, None
        return
    scratch = Path(tempfile.mkdtemp(prefix="taikyoku-engine-update-"))
    source = scratch / "source"
    registered = False
    try:
        update.command("fetch", ["git", "-C", ROOT, "fetch", "origin"])
        revision = update.command(
            "resolve-ref",
            ["git", "-C", ROOT, "rev-parse", "--verify", "--end-of-options", (args.ref or "origin/main") + "^{commit}"],
            capture=True,
        ).strip()
        update.command("checkout", ["git", "-C", ROOT, "worktree", "add", "--detach", source, revision])
        registered = True
        yield source, revision
    finally:
        if registered or source.exists():
            result = subprocess.run(["git", "-C", str(ROOT), "worktree", "remove", "--force", str(source)], check=False)
            if result.returncode:
                raise RuntimeError(f"worktree cleanup failed; retained {source}")
        shutil.rmtree(scratch)


def publish(update, engine, analyzer, revision, expected=None):
    argv = [
        sys.executable,
        ROOT / "deploy/tourney_analysis.py",
        "update-engine",
        "--run-dir",
        update.args.run,
        "--engine",
        engine,
        "--analyzer",
        analyzer,
        "--revision",
        revision,
        "--compatible-speedup",
    ]
    if expected:
        argv += ["--expected-build", expected]
    update.publication_attempted = True
    update.command("publication", argv)


def source_identity(source, output):
    # Reuse the builder's exact manifest definition. The baseline must come from
    # the same inputs as PGO, including a non-Git archive or dirty checkout.
    return pgo_build.Build(argparse.Namespace(source_dir=source, output=output), []).source_identity()


def native(args):
    update = Update(args)
    with source_tree(args, update) as (source, revision):
        target = source.parent / "target"
        env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1")
        try:
            update.command(
                "native-build",
                [ROOT / "deploy/build_native.sh", "--release", "--bin", "taikyoku_shogi", "--bin", "analyze_position"],
                cwd=source,
                env=env,
                nice=True,
            )
            publish(update, target / "release/taikyoku_shogi", target / "release/analyze_position", revision)
        except Exception as error:
            raise RuntimeError(f"native update failed during {update.stage}: {error}") from error


def pgo(args):
    config = analysis.read(args.run / "analysis/config.json")
    if not config.get("rolling_engines"):
        raise ValueError("PGO publication requires a run already configured with --rolling-engines")
    before = engine_updates.active(analysis, args.run)
    expected = analysis.digest(json.dumps(before, sort_keys=True).encode())
    output = (
        args.out
        or args.run / "analysis/pgo-builds" / f"{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}-{time.time_ns()}"
    )
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    update = Update(args, output)
    update.save(expected_build=expected, active_before=before, prepare_only=args.prepare_only)
    try:
        with source_tree(args, update) as (source, _revision):
            inputs = output / "inputs"
            argv = [
                sys.executable,
                ROOT / "deploy/prepare_pgo_corpus.py",
                "--run-dir",
                args.run,
                "--out",
                inputs,
                "--max-models",
                args.max_models,
                "--games",
                args.games,
                "--verify-games",
                args.verify_games,
                "--roots-per-game",
                args.roots_per_game,
                "--recent-games",
                args.recent_games,
            ]
            if args.models:
                argv += ["--models", *args.models]
            update.command("prepare-corpus", argv, nice=True)
            corpus = analysis.read(inputs / "corpus.json")
            if not corpus["positions"] or not corpus["models"]:
                raise ValueError("prepared training corpus has no positions or models")
            native_source = source_identity(source, output / "pgo")
            update.save(native_source=native_source)
            compiler = update.command("compiler", [os.environ.get("RUSTC", "rustc"), "-vV"], cwd=source, capture=True)
            update.save(native_compiler=compiler)
            host = next(
                (line.removeprefix("host: ") for line in compiler.splitlines() if line.startswith("host: ")), None
            )
            if not host:
                raise ValueError("rustc did not report a host target")
            target = output / "native-target"
            env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS="1")
            update.command(
                "native-build",
                [ROOT / "deploy/build_native.sh", "--release", "--example", "nnue_tournament_bench", "--target", host],
                cwd=source,
                env=env,
                nice=True,
            )
            native_binary = target / host / "release/examples/nnue_tournament_bench"
            argv = [
                ROOT / "deploy/build_pgo.sh",
                inputs / "corpus.json",
                output / "pgo",
                args.llvm_profdata,
                "--source-dir",
                source,
                "--positions",
                *range(len(corpus["positions"])),
                "--models",
                *range(len(corpus["models"])),
                "--repeats",
                args.profile_repeats,
                "--depth",
                args.depth,
                "--cpu",
                args.cpu,
            ]
            update.command("pgo-build", argv, cwd=source, env=env, nice=True)
            build_path = output / "pgo/build.json"
            build = None
            if build_path.exists():
                build = analysis.read(build_path)
                if build.get("status") != "complete" or build["source"] != native_source:
                    raise ValueError("native and completed PGO builds have different source identities")
                if build.get("rustc") != compiler:
                    raise ValueError("native and PGO builds used different compiler identities")
                revision = build["source"]["description"]
                update.save(pgo_build_sha256=hashlib.sha256(build_path.read_bytes()).hexdigest())
            else:
                revision = (output / "pgo/source-revision.txt").read_text().strip()
                if source_identity(source, output / "pgo") != native_source:
                    raise ValueError("source tree changed after the native build")
            if not revision:
                raise ValueError("PGO build did not identify its source")
            binaries = output / "pgo/bin"
            binary_hashes = {
                name: analysis.file_digest(binaries / name)
                for name in ("taikyoku_shogi", "analyze_position", "benchmark")
            }
            if build is not None and any(build["binaries"].get(name) != sha for name, sha in binary_hashes.items()):
                raise ValueError("PGO binary differs from its completed build manifest")
            update.save(binary_sha256=binary_hashes)
            update.command(
                "verification",
                [
                    sys.executable,
                    ROOT / "deploy/verify_pgo.py",
                    "--baseline",
                    native_binary,
                    "--candidate",
                    binaries / "benchmark",
                    "--corpus",
                    inputs / "verify-corpus.json",
                    "--out",
                    output / "verification",
                    "--cpu",
                    args.cpu,
                    "--repeats",
                    args.verify_repeats,
                    "--depth",
                    args.depth,
                    "--require-speedup",
                ],
                cwd=source,
                nice=True,
            )
            update.save(
                revision=revision, engine=str(binaries / "taikyoku_shogi"), analyzer=str(binaries / "analyze_position")
            )
            if any(analysis.file_digest(binaries / name) != sha for name, sha in binary_hashes.items()):
                raise ValueError("PGO binary changed during verification")
            if not args.prepare_only:
                publish(update, binaries / "taikyoku_shogi", binaries / "analyze_position", revision, expected)
        update.save(status="prepared" if args.prepare_only else "published", finished=time.time())
        print(
            f"PGO {'prepared without publication' if args.prepare_only else 'published'}; evidence={output}", flush=True
        )
    except BaseException as error:
        update.save(status="failed", error=str(error), publication_attempted=update.publication_attempted)
        if update.publication_attempted:
            effect = "Inspect active-engine.json/status before retrying; no restart was requested."
        else:
            effect = "No engine publication was attempted; the tournament was not restarted."
        raise RuntimeError(f"PGO update failed during {update.stage}: {error}. {effect}") from error


def main(argv=None):
    args = parser().parse_args(argv)
    validate_args(args)
    args.run = args.run.resolve(strict=True)
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    if args.pgo:
        pgo(args)
    else:
        native(args)


if __name__ == "__main__":
    try:
        main()
    except Exception as exc:
        print(f"Engine update failed: {exc}; no tournament restart was requested.", file=sys.stderr)
        sys.exit(1)
