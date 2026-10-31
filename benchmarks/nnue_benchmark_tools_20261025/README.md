# NNUE benchmark-tool validation — 2026-10-25

This validates the measurement tools, not a production speed optimization. The reference and candidate training arithmetic is unchanged in the W512 control: `pilot_fit.py` and `train.py` have identical loaded hashes in both arms. Timing ratios here illustrate local variability rather than delivered gains.

- Reference revision: `cefe79608dde9d10bcf4a0afd3a28f2f88a8cdd8`
- Candidate benchmark revision: `760d53175265912fcbde79b3413054c32523201e` (clean training source)
- Host: 12th Gen Intel(R) Core(TM) i7-1255U, Linux, CPU affinity 2, one PyTorch thread
- Python 3.12.14, PyTorch 2.14.0+cpu, NumPy 2.5.2
- Frozen pilot corpus: 71,389 records; input component hashes verified before timing; dataset identity recorded in each JSON file.
- W512: three alternating-order process pairs, seed 20261025 + pair index; each process has one warmup update and eight timed batch-8/microbatch-1 updates (64 draws), followed by 64 validation records.
- `--artifacts` exports and writes a benchmark checkpoint in private temporary storage. `--profile` adds two separate processes using the first seed; these never enter the normal timing statistics.

## W512 unchanged-trainer control

Each duration row reports median [min, max] across three ordinary process observations per arm. Relative figures use median paired ratios, not a quotient of separate medians. RSS includes the complete worker lifetime; profile runs are excluded.

| Metric | Reference median [min, max] | Candidate median [min, max] | Paired ratio median [min, max] |
|---|---:|---:|---:|
| train_s (seconds) | 1.2589 [1.1947, 1.3890] | 1.1928 [1.1555, 1.2225] | 0.9179 [0.8587, 1.0232] |
| validation_s (seconds) | 0.2010 [0.1958, 0.2308] | 0.1934 [0.1838, 0.2142] | 0.9143 [0.8378, 1.0937] |
| workload_s (seconds) | 2.1262 [2.0896, 2.6188] | 2.0698 [2.0526, 2.1243] | 0.9654 [0.7903, 1.0166] |
| process_s (seconds) | 8.1669 [8.0197, 11.7562] | 8.2806 [7.8643, 9.1212] | 0.9629 [0.7759, 1.0325] |
| Peak RSS (MiB) | 1110.8477 [1108.5391, 1148.0312] | 1125.8789 [1122.5625, 1139.6680] | 1.0127 [0.9807, 1.0259] |
| export_s (seconds) | 0.4108 [0.3979, 0.5414] | 0.4124 [0.4089, 0.4422] | 1.0039 [0.7553, 1.1113] |
| checkpoint_s (seconds) | 0.2883 [0.2683, 0.4575] | 0.2747 [0.2710, 0.2753] | 0.9550 [0.6003, 1.0101] |

All three pairs matched sampled IDs, dataset identity, losses, complete validation metrics, residual scores and final parameter SHA-256. Both instrumented processes matched their corresponding first-seed ordinary run, and all nine training callable groups were available. Cumulative profile spans overlap and are not an additive time partition.

The profile records 64 packing/forward/objective/backward/coalescing calls, 16 gradient resets, eight sparse and eight dense optimizer steps, and eight clamp calls per training window. Exact counts are in `control.json`.

Each W512 artifact sample wrote 244,223,428 quantized-model bytes and 488,796,341 checkpoint bytes, then removed the private files. The benchmark checkpoint omits production best-state/recipe/admission metadata and is not a resumable training output.

## Compatibility and numerical screening

`legacy.json` compares the original default baseline `6b3397c0409a48bd1fa2c82deb4888b4a26c39f6` with candidate `760d53175265912fcbde79b3413054c32523201e`, using width 32, one ordinary pair, two updates and eight validation records. It also has two separate profile processes. All comparisons were exact. This small compatibility smoke does not establish a speed gain.

`microbatch.json` exercises the explicit numerical-screening mode on the same real corpus, W512, one pair, four updates (32 draws), 32 validation records, and effective batch eight with microbatch 1 versus 8. It reports `numerical_differences_allowed`, not `exact`: weights differ, maximum update-loss error is 0.000340417522, and maximum residual-score difference is 0.218364716 engine units. This single pair demonstrates diagnostic handling and is not a performance/quality adoption result. The automated negative tests separately verify that the default mode fails after saving such a mismatch, and that different sampled IDs or dataset identities cannot be waived. They also cover timeouts, interruption, output preservation, sharded corruption and profile parity.

## Reproduce the control

```sh
data/nnue-venv/bin/python training/nnue/benchmark_training.py \
  data/nnue-pilot-20260924/dataset \
  --baseline-rev cefe79608dde9d10bcf4a0afd3a28f2f88a8cdd8 \
  --widths 512 --pairs 3 --steps 8 --validation-samples 64 \
  --seed 20261025 --cpu 2 --profile --artifacts \
  --out /tmp/NEW-nnue-benchmark-control.json
```

The whole workload window excludes input hashing, model setup/warmup, the extra validation-score diagnostic pass and parameter/source hashing; `process_s` includes those costs. No other build, test or profiling job ran during ordinary measurements. Host power/frequency and unrelated host activity were not controlled. Three local pairs provide a screening range, not a robust confidence interval, individual-update p95/p99, full-epoch throughput, resumed v5 training, GPU performance, or playing-strength evidence.
