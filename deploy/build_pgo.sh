#!/usr/bin/env bash
# Build local host binaries with a compiler profile from existing tournament data.
# Does not train NNUE weights, publish binaries, or start/restart a tournament.
set -Eeuo pipefail
trap 'echo "PGO build failed at line $LINENO; no tournament was started or updated." >&2' ERR
if [[ $# -lt 3 ]]; then
  echo "usage: $0 CORPUS_JSON NEW_OUTPUT_DIR LLVM_PROFDATA --positions N... --models N... [--repeats N]" >&2
  exit 2
fi
corpus=$(realpath "$1")
output=$(realpath -m "$2")
profdata=$(realpath "$3")
shift 3
repo=$(cd "$(dirname "$0")/.." && pwd)
if [[ -e "$output" ]]; then
  echo "PGO output already exists: $output; choose a new directory. No tournament was started or updated." >&2
  exit 2
fi
rust_llvm=$(rustc -Vv | sed -n 's/^LLVM version: //p')
tool_llvm=$("$profdata" --version | sed -n 's/.*LLVM version \([^ ]*\).*/\1/p')
tool_llvm=${tool_llvm%%-*}
if [[ -z "$rust_llvm" || "$rust_llvm" != "$tool_llvm" ]]; then
  echo "LLVM version mismatch: rustc=$rust_llvm, llvm-profdata=$tool_llvm. No tournament was started or updated." >&2
  exit 2
fi
mkdir -p "$output/bin"
cd "$repo"
export CARGO_TARGET_DIR="$output/target"
# Cargo splits ordinary RUSTFLAGS on whitespace; encoded flags preserve paths.
if [[ -n ${CARGO_ENCODED_RUSTFLAGS+x} ]]; then
  base_flags=$CARGO_ENCODED_RUSTFLAGS
else
  base_flags=$(python3 -c 'import os; print("\x1f".join(os.environ.get("RUSTFLAGS", "").split()), end="")')
fi
base_flags=${base_flags:+$base_flags$'\x1f'}$'-C\x1ftarget-cpu=native'
export CARGO_ENCODED_RUSTFLAGS=$base_flags$'\x1f-C\x1fprofile-generate='"$output/raw"
cargo build --locked --release --example nnue_tournament_bench
cp "$CARGO_TARGET_DIR/release/examples/nnue_tournament_bench" "$output/bin/instrumented"
python3 benchmarks/nnue_followup/profile_workload.py --binary "$output/bin/instrumented" \
  --corpus "$corpus" --out "$output/workload" "$@"
"$profdata" merge -o "$output/merged.profdata" "$output"/workload/profiles/*.profraw
export CARGO_ENCODED_RUSTFLAGS=$base_flags$'\x1f-C\x1fprofile-use='"$output/merged.profdata"$'\x1f-C\x1fllvm-args=-pgo-warn-missing-function'
cargo build --locked --release --bin taikyoku_shogi --bin analyze_position --example nnue_tournament_bench
cp "$CARGO_TARGET_DIR/release/taikyoku_shogi" "$output/bin/taikyoku_shogi"
cp "$CARGO_TARGET_DIR/release/analyze_position" "$output/bin/analyze_position"
cp "$CARGO_TARGET_DIR/release/examples/nnue_tournament_bench" "$output/bin/benchmark"
git rev-parse HEAD > "$output/source-revision.txt"
git diff --binary > "$output/source-diff.patch"
sha256sum "$output"/bin/* > "$output/binary-sha256.txt"
echo "PGO build complete: $output/bin; no tournament was started or updated."
