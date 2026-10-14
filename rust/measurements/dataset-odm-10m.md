# Ten-million-row ODM execution timing

The [raw observation](dataset-odm-10m-2026-10-14.json) records three optimized
Rust executions of 10,000,000 synthetic ODM item rows. The median elapsed time was
**9.352 seconds**, or **1.069 million input rows/second**; individual trials took
9.779, 9.352 and 8.847 seconds. These are measurements of one defined workload,
not a runtime estimate for every ODM domain or a release performance gate.

The source has all eleven canonical ODM string columns, 10,000 subjects and
1,000 items per subject, encoded as 853,927,624 bytes of in-memory IPC in chunks
of 65,536 rows. Two item reads (`IT.DM.AGE` and `IT.DM.SEX`) derive AGE and SEX in
key scope, grouping by study/subject into 10,000 output rows. AGE conversion,
complete output-key checks and an exact output row-count verification are
included. Every output key and value is checked independently after timing.
The unmodified `ExecutionBudget::longitudinal()` profile admits this workload.

| Timed step | Median seconds |
| --- | ---: |
| Canonical IPC preflight, decode, normalization and logical-text accounting | 2.537 |
| Key construction, ODM item reads, conversion, output keys and row-count verification | 6.814 |
| Sanitized IPC export to a byte-count sink | 0.000855 |
| Complete decode/execution/export interval | 9.352 |

Each median is calculated independently; step medians do not sum to the total
median. Compilation, synthetic input generation (0.688 seconds), XML/CSV parsing,
specification compilation, host dispatch, disk/network publication and the
post-run expected-value checks are outside the timed interval. Output IPC is
336,136 bytes. Peak process RSS was **3,398,021,120 bytes (3.165 GiB)**, including
input generation and all three trials; it is not a per-step memory measurement.

The machine was a Linux x86_64 container on an AMD EPYC 9V74, with a quota of four
CPU cores and a 16 GiB memory limit. The example executes on one calling thread.
It used Rust 1.90.0 and the release profile (optimization level 3, thin LTO, debug
assertions disabled). Source revision and every raw sample are retained in the
JSON record. Fresh source decoding and engine counters are used for each run,
while the encoded input remains in memory.

Reproduce from `rust/`:

```sh
cargo run --locked --offline --release -p yamaa-adapters \
  --example odm_budget_benchmark -- \
  --subjects 10000 --items-per-subject 1000 --runs 3
```

Omit `--offline` if locked dependencies are not already cached. A small correctness
probe uses `--subjects 3 --items-per-subject 5 --runs 1`.

This timing uses key scope, whose ODM reads scan the records feeding each subject.
Record and group scopes can scan the complete source for each output row; they
can run much more slowly or exhaust the work quota. More derived items, predicates,
windows, different subject/record distributions and larger strings also change
runtime and capacity. Chunked transport does not remove the engine's retained
source and key-construction data. A real domain specification and its source
format are needed to time a complete XML/CSV-to-artifact workflow.
