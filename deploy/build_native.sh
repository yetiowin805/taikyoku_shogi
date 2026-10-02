#!/usr/bin/env bash
# Run from the source checkout to build binaries for this machine's CPU.
# Use ordinary `cargo build` when producing a portable binary for another host.
set -Eeuo pipefail
trap 'echo "Native engine build failed at line $LINENO; the tournament did not start." >&2' ERR
if [[ -n ${CARGO_ENCODED_RUSTFLAGS+x} ]]; then
  export CARGO_ENCODED_RUSTFLAGS=${CARGO_ENCODED_RUSTFLAGS:+$CARGO_ENCODED_RUSTFLAGS$'\x1f'}$'-C\x1ftarget-cpu=native'
else
  export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C target-cpu=native"
fi
cargo build --locked "$@"
