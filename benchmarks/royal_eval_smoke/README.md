# A/L sanity and cost probes

Opt-in prototypes on `68dada7`, built with `royal-probes`. They do not change checkpoints, normal builds, live tournament configuration, or production defaults. This is a small first filter, not a strength trial or a validation of the planned swap/aspiration baseline. Search measurements force S2 on and otherwise use the recorded checkpoint search policy.

## Prototypes

- A40/A80/A160: existing geometric alignment, with cap five times the weight.
- Ablocked: existing 80/400 alignment but requiring an empty first-leg ray to the royal. Conservative and cheap; this deliberately does not enumerate all legal two-leg attacks or recognize capture-through-blocker routes.
- Lold: existing last-royal flight penalty, weight 4000.
- Lverified: activate only when the last royal is actually checked; enumerate the royal's real movement targets, include captures, simulate each landing and test safety with the engine's check rules. Stop after two safe destinations. This includes two-square King movement; Crown Prince is one square. No full army move generation and no board clones in static evaluation.
- Defense scarcity: consume the existing evasion list; one defense maps to 1000 and two to 400. Zero is left to terminal handling. This is a diagnostic/cost prototype, not wired into search scores: adding it to a solved check node could double-count a threat and needs a separate experiment.
- Mating check probe: side-to-move checking moves with no legal evasion. At most 64 attack candidates, 256 defense candidates, and a two-millisecond deadline checked around generation and simulations. Found requires a complete proof; partial work is Unknown. A private full-state clone prevents production undo's piece-list reordering from leaking back. Generation itself is not interruptible, so this is a soft deadline. Direct royal captures are outside its scope. This is a root-only diagnostic, not an evaluation term; applying it with the opponent to move would establish a conditional threat, not forced mate against an intervening defense.

## Reproduce

Build and run correctness tests before measuring. Run the three phases sequentially on one available pinned CPU, with no concurrent build:

```sh
cargo test --locked --features royal-probes --lib eval::
cargo build --locked --release --features royal-probes --example royal_probe_smoke
taskset -c 0 target/release/examples/royal_probe_smoke describe CORPUS.json > describe.jsonl
taskset -c 0 target/release/examples/royal_probe_smoke micro CORPUS.json > micro.jsonl
timeout 480s taskset -c 0 target/release/examples/royal_probe_smoke search CORPUS.json > search.jsonl
python3 benchmarks/royal_eval_smoke/summarize.py OUTPUT_DIRECTORY
```

A corpus is an array of `{name, game, ply, model}`. `ply` is the number of applied moves, not the next one-based move number. The harness replays the complete prefix. The smoke selection uses five pre-existing corpus positions: opening, middle, reversal, late and a 1,031-ply reversal; all original file hashes are retained privately with the results. Six synthetic fixtures exercise capture escape, discovered ray, two-square escape, interposition, an incoming mating check and an unthreatened boxed royal.

Micro measurements use five shuffled repetitions, at least 25 ms per position/variant, reporting nanoseconds per complete incremental evaluation. Timed searches use two shuffled repetitions at three seconds on the five recorded positions and two nonterminal forced fixtures. S2 is enabled throughout; A/L are reset before each variant. Full move routes, root lines, scores, nodes and completed depth are saved. Timed aggregate estimates exclude depth-cap/terminal runs finishing before 2.9 seconds. Small NPS differences are noise-sensitive, and changed search trees mean they are not equal-work speedups.

## Sanity findings

Existing L counts only empty neighboring squares. The engine's King actually moves up to two squares, and Crown Prince moves one. The fixtures demonstrate that the old count can miss both two-square escapes and safe captures, and can count a square made unsafe by vacating the royal's current square. Even the verified flight count is not a complete trapped-royal diagnosis: a non-royal interposition can save a royal with zero flights.

The diagnostic probes preserve the input position and traversal order by keeping make/unmake on a private clone. Probe tests also cover color symmetry, a spare royal, exhausted budgets returning Unknown, positive mate proof, evaluation parity and inherited repetition/history preservation.

## Initial local results

43 debug evaluation tests passed (nine new), and the default library build passed. Five saved positions × seven settings × two repetitions, plus two forced fixtures, produced 98 search records. Six synthetic fixtures and the five saved positions produced 385 evaluation timing blocks.

Equal-position NPS changes versus A/L off with S2 on: A40 −6.27%, A80 −4.49%, A160 −3.92%, blocker-aware A −3.89%, old L −9.68%, verified L −5.62%. Completed depth did not change on the five saved positions. Each L version changed the move in two of ten real-position observations; A versions changed none. This small sample cannot rank strength or reliably resolve small timing differences.

Blocker-aware A adds little cost over existing A and passes the basic filter checks. Verified L fixes concrete feature-detection mistakes, but still exceeds the proposed cheap-default cost gate. In the late-game case, old L reduced NPS by about 31%, verified L by about 10%. Reused defense-count scoring is not integrated yet. The root mating-check diagnostic was inexpensive (up to 0.451 ms in the wide-route stress sample) but returned Unknown on applicable real positions; no useful real-position positive detection has been established.

For additional wide Tengu/Free Eagle generation checks, run `probe-stress CORPUS.json` through the same executable. The final diagnostic also skips roots already drawn by repetition or the progress rule. The exact timing build and raw results are preserved privately; diagnostic draw guards and stress mode were added after timing and do not alter the A/L evaluation functions. These are measurements against main plus S2, not the planned swap/aspiration engine. No tournament was launched.
