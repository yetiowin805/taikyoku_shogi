#!/usr/bin/env python3
"""Build host-local PGO binaries from one recorded source tree; never publish them."""

import argparse
import contextlib
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEPARATOR = "\x1f"


def file_digest(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def atomic_json(path, value):
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("w") as output:
        output.write(json.dumps(value, indent=2) + "\n")
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(path)
    fd = os.open(path.parent, os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def interrupted(signum, _frame):
    raise RuntimeError(f"PGO interrupted by signal {signum}")


class Build:
    def __init__(self, args, workload):
        self.args = args
        self.workload = workload
        self.output = args.output.resolve()
        self.source = args.source_dir.resolve(strict=True)
        self.stage = "preflight"
        self.metadata = dict(
            format_version=1,
            status="running",
            source_dir=str(self.source),
            output_dir=str(self.output),
            started=time.time(),
            commands=[],
        )
        self.created = False

    def save(self):
        if self.created:
            self.metadata.update(stage=self.stage, updated=time.time())
            atomic_json(self.output / "build.json", self.metadata)

    def command(self, argv, *, env=None, capture=False):
        argv = list(map(str, argv))
        record = dict(argv=argv, cwd=str(self.source), stage=self.stage, started=time.time())
        if env is not None:
            record["environment"] = {
                key: env[key]
                for key in ("CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC")
                if key in env
            }
        self.metadata["commands"].append(record)
        self.save()
        proc = subprocess.Popen(
            argv,
            cwd=self.source,
            env=env,
            stdout=subprocess.PIPE if capture else None,
            text=True,
            start_new_session=True,
        )
        try:
            stdout, _ = proc.communicate()
        finally:
            # The direct child may already have exited while a compiler or
            # linker descendant still owns its pipes. Reap the whole session.
            with contextlib.suppress(ProcessLookupError):
                os.killpg(proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            with contextlib.suppress(ProcessLookupError):
                os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()
            record.update(returncode=proc.returncode, finished=time.time())
            if capture:
                record["stdout"] = stdout if "stdout" in locals() else None
            self.save()
        if proc.returncode:
            raise RuntimeError(f"command exited {proc.returncode}: {argv}")
        return stdout if capture else None

    def source_identity(self):
        git = shutil.which("git")
        is_git = False
        if git:
            probe = subprocess.run(
                [git, "-C", str(self.source), "rev-parse", "--show-toplevel"],
                capture_output=True,
                text=True,
                check=False,
            )
            is_git = probe.returncode == 0 and Path(probe.stdout.strip()).resolve() == self.source
        revision = None
        diff = None
        if is_git:
            revision = self.command([git, "rev-parse", "HEAD"], capture=True).strip()
            # HEAD includes both staged and unstaged tracked changes.
            diff = self.command([git, "diff", "--binary", "HEAD", "--"], capture=True)
            names = self.command([git, "ls-files", "-z", "--cached", "--others", "--exclude-standard"], capture=True)
            paths = sorted(set(names.rstrip("\0").split("\0"))) if names else []
            # Include all tracked files plus nonignored untracked compiler inputs.
            tracked = set(self.command([git, "ls-files", "-z"], capture=True).rstrip("\0").split("\0"))
            paths = [
                name
                for name in paths
                if name in tracked
                or Path(name).suffix == ".rs"
                or Path(name).name in ("Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain", "rust-toolchain.toml")
                or name.startswith(("src/", "examples/", ".cargo/"))
            ]
        else:
            paths = []
            for directory, subdirs, files in os.walk(self.source, followlinks=False):
                parent = Path(directory)
                subdirs[:] = sorted(
                    name
                    for name in subdirs
                    if name not in (".git", "target") and not (parent / name).is_relative_to(self.output)
                )
                for name in subdirs:
                    if (parent / name).is_symlink():
                        raise ValueError(f"source directory symlinks are not supported: {parent / name}")
                paths.extend(str((parent / name).relative_to(self.source)) for name in sorted(files))
        files = {}
        for name in paths:
            path = self.source / name
            if path.is_relative_to(self.output) or ".git" in path.relative_to(self.source).parts:
                continue
            if path.is_relative_to(self.source / "target"):
                continue
            if not path.exists() and not path.is_symlink():
                # A tracked deletion is represented by git diff, not a file hash.
                continue
            target = path.resolve(strict=True)
            if not target.is_relative_to(self.source):
                raise ValueError(f"source symlink escapes source tree: {path}")
            if not path.is_file():
                raise ValueError(f"source input is not a regular file: {path}")
            files[name] = dict(sha256=file_digest(path), bytes=path.stat().st_size)
            if path.is_symlink():
                files[name]["symlink"] = os.readlink(path)
        canonical = json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
        manifest_hash = hashlib.sha256(canonical).hexdigest()
        return dict(
            kind="git" if is_git else "archive",
            revision=revision,
            diff=diff,
            diff_sha256=hashlib.sha256(diff.encode()).hexdigest() if diff is not None else None,
            files=files,
            manifest_sha256=manifest_hash,
            description=revision if is_git else "archive-sha256:" + manifest_hash,
        )

    def discover_profdata(self, rustc, rust_info):
        expected = re.search(r"^LLVM version: (\S+)", rust_info, re.MULTILINE)
        host = re.search(r"^host: (\S+)", rust_info, re.MULTILINE)
        if not expected or not host:
            raise ValueError("rustc -Vv did not report LLVM version and host")
        expected = expected.group(1)
        host = host.group(1)
        candidates = []
        if self.args.profdata != "auto":
            explicit = shutil.which(self.args.profdata)
            candidates.append(Path(explicit or self.args.profdata).resolve())
        else:
            sysroot = self.command([rustc, "--print", "sysroot"], capture=True).strip()
            candidates.append(Path(sysroot) / "lib/rustlib" / host / "bin/llvm-profdata")
            major = expected.split(".")[0]
            for name in ("llvm-profdata", "llvm-profdata-" + expected, "llvm-profdata-" + major):
                path = shutil.which(name)
                if path:
                    candidates.append(Path(path).resolve())
        rejected = []
        for candidate in dict.fromkeys(candidates):
            if not candidate.is_file() or not os.access(candidate, os.X_OK):
                rejected.append(f"{candidate}: missing executable")
                continue
            try:
                version = self.command([candidate, "--version"], capture=True)
            except RuntimeError as exc:
                rejected.append(str(exc))
                continue
            match = re.search(r"LLVM version\s+(\d+\.\d+\.\d+)", version)
            if match and match.group(1) == expected:
                self.metadata["llvm"] = dict(
                    version=expected, profdata=str(candidate), profdata_sha256=file_digest(candidate)
                )
                return candidate, host
            rejected.append(f"{candidate}: {version.strip()}")
        raise ValueError(
            f"no llvm-profdata matching rustc LLVM {expected}: {'; '.join(rejected)}. "
            "Install llvm-tools-preview for the active Rust toolchain, or pass a matching executable"
        )

    def run(self):
        if self.output.exists():
            raise ValueError(f"PGO output already exists: {self.output}; choose a new directory")
        if not (self.source / "Cargo.toml").is_file():
            raise ValueError(f"source directory has no Cargo.toml: {self.source}")
        self.output.mkdir(parents=True)
        self.created = True
        (self.output / "bin").mkdir()
        self.save()
        self.stage = "source identity"
        before = self.source_identity()
        self.metadata["source"] = before
        (self.output / "source-revision.txt").write_text(before["description"] + "\n")
        (self.output / "source-diff.patch").write_text(before["diff"] or "")
        self.stage = "LLVM discovery"
        rustc = os.environ.get("RUSTC", "rustc")
        self.metadata["rustc"] = self.command([rustc, "-Vv"], capture=True)
        self.metadata["cargo"] = self.command(["cargo", "-V"], capture=True)
        profdata, host = self.discover_profdata(rustc, self.metadata["rustc"])
        if os.environ.get("CARGO_BUILD_TARGET", host) != host:
            raise ValueError("host-local PGO does not support a different CARGO_BUILD_TARGET")
        base = (
            os.environ["CARGO_ENCODED_RUSTFLAGS"].split(SEPARATOR) if os.environ.get("CARGO_ENCODED_RUSTFLAGS") else []
        )
        if "CARGO_ENCODED_RUSTFLAGS" not in os.environ:
            base = os.environ.get("RUSTFLAGS", "").split()
        if any("profile-generate" in flag or "profile-use" in flag for flag in base):
            raise ValueError("caller flags already contain PGO options")
        base.extend(["-C", "target-cpu=native"])
        self.metadata["flags"] = base
        self.metadata["host"] = dict(
            target=host,
            allowed_cpus=sorted(os.sched_getaffinity(0)),
            platform=dict(zip(("system", "node", "release", "version", "machine"), os.uname(), strict=True)),
        )
        corpus = self.args.corpus.resolve(strict=True)
        corpus_hash = file_digest(corpus)
        runner = ROOT / "benchmarks/nnue_followup/profile_workload.py"
        self.metadata["corpus"] = dict(path=str(corpus), sha256=corpus_hash)
        self.metadata["tooling"] = {
            str(path): file_digest(path) for path in (Path(__file__).resolve(), ROOT / "deploy/build_pgo.sh", runner)
        }
        env = dict(os.environ, CARGO_TARGET_DIR=str(self.output / "target"))
        env.setdefault("CARGO_BUILD_JOBS", "1")
        generate = base + ["-C", "profile-generate=" + str(self.output / "raw")]
        env["CARGO_ENCODED_RUSTFLAGS"] = SEPARATOR.join(generate)
        cargo = ["cargo", "build", "--locked", "--release", "--target", host]
        release = self.output / "target" / host / "release"
        self.stage = "instrumented build"
        self.command(cargo + ["--example", "nnue_tournament_bench"], env=env)
        instrumented = self.output / "bin/instrumented"
        shutil.copy2(release / "examples/nnue_tournament_bench", instrumented)
        instrumented_hash = file_digest(instrumented)
        self.stage = "profile workload"
        self.command(
            [
                sys.executable,
                runner,
                "--binary",
                instrumented,
                "--corpus",
                corpus,
                "--out",
                self.output / "workload",
                *self.workload,
            ],
            env=env,
        )
        plan = json.loads((self.output / "workload/plan.json").read_text())
        if plan["status"] != "complete" or plan["corpus_sha256"] != corpus_hash:
            raise ValueError("profile plan did not complete with the selected corpus")
        if plan.get("binary") != str(instrumented) or plan.get("binary_sha256") != instrumented_hash:
            raise ValueError("profile plan did not use the freshly built instrumented binary")
        self.metadata["workload"] = plan
        profiles = sorted((self.output / "workload/profiles").glob("*.profraw"))
        if not profiles:
            raise ValueError("profile workload produced no raw profiles")
        self.metadata["profiles"] = {str(path): file_digest(path) for path in profiles}
        self.stage = "profile merge"
        merged = self.output / "merged.profdata"
        self.command([profdata, "merge", "-o", merged, *profiles])
        if merged.stat().st_size == 0:
            raise ValueError("profile merge produced an empty profile")
        self.metadata["merged_profile_sha256"] = file_digest(merged)
        self.stage = "profile-use build"
        env["CARGO_ENCODED_RUSTFLAGS"] = SEPARATOR.join(
            base + ["-C", "profile-use=" + str(merged), "-C", "llvm-args=-pgo-warn-missing-function"]
        )
        self.command(
            cargo + ["--bin", "taikyoku_shogi", "--bin", "analyze_position", "--example", "nnue_tournament_bench"],
            env=env,
        )
        for name, source in (
            ("taikyoku_shogi", release / "taikyoku_shogi"),
            ("analyze_position", release / "analyze_position"),
            ("benchmark", release / "examples/nnue_tournament_bench"),
        ):
            shutil.copy2(source, self.output / "bin" / name)
        self.stage = "final identity verification"
        if self.source_identity() != before:
            raise ValueError("source tree changed during PGO build")
        if file_digest(corpus) != corpus_hash:
            raise ValueError("corpus changed during PGO build")
        # The runner audited input hashes before and after profiling. Verify them
        # again after compilation so the final provenance cannot describe drift.
        for path, entry in plan["inputs"].items():
            if file_digest(path) != entry["sha256"]:
                raise ValueError(f"profile input changed during PGO build: {path}")
        for field in ("tooling", "profiles"):
            for path, digest in self.metadata[field].items():
                if file_digest(path) != digest:
                    raise ValueError(f"PGO {field} changed during build: {path}")
        if file_digest(merged) != self.metadata["merged_profile_sha256"]:
            raise ValueError("merged profile changed during build")
        self.metadata["binaries"] = {path.name: file_digest(path) for path in sorted((self.output / "bin").iterdir())}
        (self.output / "binary-sha256.txt").write_text(
            "".join(f"{digest}  {self.output / 'bin' / name}\n" for name, digest in self.metadata["binaries"].items())
        )
        self.metadata.update(status="complete", finished=time.time())
        self.save()
        print(f"PGO build complete: {self.output}/bin; no tournament was started or updated.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("profdata", help="llvm-profdata executable or auto")
    parser.add_argument("--source-dir", type=Path, default=ROOT)
    args, workload = parser.parse_known_args()
    for signum in (signal.SIGTERM, signal.SIGINT):
        signal.signal(signum, interrupted)
    build = None
    try:
        build = Build(args, workload)
        build.run()
    except BaseException as exc:
        stage = build.stage if build else "preflight"
        print(f"PGO build failed during {stage}: {exc}; no tournament was started or updated.", file=sys.stderr)
        if build:
            build.metadata.update(status="failed", error=str(exc), finished=time.time())
            try:
                build.save()
            except OSError as save_error:
                print(f"Could not save failed PGO build metadata: {save_error}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
