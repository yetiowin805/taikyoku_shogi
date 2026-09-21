# Production NNUE acceleration

Normal builds enable the four exact accelerations tested in PR #107:

- Cache summed ability vectors by complete piece-feature identity, square and perspective. Each accumulator owns at most 4096 entries; clones begin with empty caches.
- Save accumulator sums on make and restore them on undo, recycling buffers. Snapshots bind their original network; rebinding a state between make and undo invalidates the stale accumulator.
- Compute the first dense layer with exact AVX2 unsigned-byte/signed-byte dot products when supported, otherwise use the scalar implementation. Network weights, clipping, rounding and floating-point accumulation order are unchanged.
- Temporarily remove the accumulator during rule-only royal-safety simulations and restore the same object afterward, including failed moves.

There are no runtime experimental flags, pruning plans, residual memoization or network changes in this PR. These are implementation changes, with fixed-depth search equivalence as the acceptance criterion. Faster timed searches can finish additional work and choose differently.

The original post-merge study compared the full bundle with main `05b6ec7`, with native compilation held constant: 1.39x / 2.34x equal-position NPS at widths 512 / 2048. Those are small-corpus local measurements, not VPS or Elo predictions. The production integration is checked again here against the original baseline and the tested prototype.

## Reproduce

The reference binaries and immutable games/models are the content-pinned outputs of PR #107's post-merge study. `check.py` checks their identities before running. The verification phase compares all five trained NNUE widths plus two handcrafted checkpoints, on four complete game prefixes, at depth 2. It requires exact scores, complete chosen routes, ordered root lines and main/quiescence node counts. Timing compares this production integration to the already-tested full prototype in two paired three-second repetitions at widths 512/2048.

```sh
cargo test --locked --lib
RUSTFLAGS='-C target-cpu=native' cargo test --locked --release
RUSTFLAGS='-C target-cpu=native' cargo build --locked --release --bins --example nnue_speed_check
python3 -m unittest discover -s deploy -p 'test_*.py'
# Use the existing NNUE training environment (NumPy + PyTorch):
/path/to/nnue-venv/bin/python -m unittest discover -s training/nnue -p 'test_*.py'
ROOT=/home/frank/taikyoku_shogi
OUT="$ROOT/data/nnue-production-20260921"
for phase in verify timed; do
  python3 benchmarks/nnue_production/check.py --root "$ROOT" --out "$OUT" \
    --candidate "$PWD/target/release/examples/nnue_speed_check" --phase "$phase" || break
done
```

Finish all builds and tests before timing. Runs are sequential on CPU 0; loading, replay and a depth-1 TT warmup are outside the measured search. Raw rows include full moves and root lines. Manifests record model/blob identities, game/source/binary hashes, compiler settings and randomized order. Use a new output directory for a repeat; existing result files are never overwritten.

For deployment, rebuild on the destination CPU with `RUSTFLAGS='-C target-cpu=native'`, preserve the coordinator's current run/state/models/sidecar setting, stop through its supported CLI, and resume without regenerating a grid or rebaking weights. Existing historical-engine bindings remain pinned. Keep the old frozen executable and configuration available for rollback.
