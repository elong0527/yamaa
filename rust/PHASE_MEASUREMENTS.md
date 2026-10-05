# Prototype phase measurements

This is a measurement prerequisite for issue #1585, step 6. It does not enable
another operation or qualify full Rust execution. Python remains the default and
both hosts continue to report `execution_supported=false`.

## Boundaries

The engine emits clock-free stage transitions. The optional adapter owns a
monotonic clock and records non-overlapping durations for a single actual attempt:

| Phase | Included work |
| --- | --- |
| `request_admission` | JSON decoding, typed plan admission and compilation |
| `host_bindings` | Callable/signature validation, captured host bindings, input byte/count admission |
| `snapshot_decode` | Owned Arrow snapshot decoding and validation |
| `engine_admission` | Handler registration, schema and row-count admission |
| `derivation` | Source reads, filtering, assignment, grouping, windows, callbacks and per-value result conversion |
| `output_keys` | Dataset materialization and output identity checks |
| `verification` | Dataset verification and final observation accounting |
| `response_encoding` | Dataset IPC, outcome JSON, and Python byte-object construction |

`_profile_dataset_functions` is a private installed Python measurement entrypoint
with the same explicit callback authority as `execute_dataset_functions`. It
returns the original table and outcome plus `dataset-profile/1` timing JSON.
Durations and their exact sum are decimal nanosecond strings. Timing metadata is
not a language observation, diagnostic, capability or cache identity. Metrics
serialization and the outer Python call overhead are outside the native total.
The ordinary path does not create a clock.

A semantic failure closes its reached stage and encodes its ordinary outcome;
unreached phases are omitted. Invalid transport, resource/host errors and original
Python control-flow exceptions still propagate without a successful timing result.
Observers do not retry callbacks, reread sources or publish intermediate values.

## Reproduce with installed packages

Build a release native wheel and the current ordinary Python wheel, install both
in an isolated environment, and run from outside the source tree. For example,
with absolute paths substituted for the environment and checkout:

```sh
/path/to/venv/bin/python -I /path/to/yamaa/rust/tools/measure_dataset_phases.py \
  --root /path/to/yamaa --repeats 5 --output /tmp/dataset-phases.json
```

The script imports installed packages; the checkout supplies unchanged fixtures
and schema resources. Every sample starts a fresh isolated interpreter. Project
activation caching is disabled. It rotates reference, ordinary native and
instrumented native mode order across repeats and retains every raw observation.
It executes the original ordered ADLB sum, window-functions, lookup and project
functions benchmarks. Every sample must succeed and exactly match the committed
CSV bytes. It never updates expected output.

`process_wall` includes process startup, script imports, package imports, the
sample, truth comparison, metadata serialization and process shutdown. It is not
an execution-only number. `package_imports` measures the selected backend's
imports; it does not measure interpreter startup. `load_normalize_validate` and
`render` are separate host boundaries. `run_total` includes source ingestion,
activation where applicable, planning, conversion, execution, verification and
artifact construction. Native-profile mode additionally measures host admission,
planning/lowering, activation, Arrow encoding, output materialization and artifact
construction. These are components of `run_total`, not extra costs to add to it.
Native phase durations are nested inside `native_call`, which also includes the
Python boundary and metrics decoding. Uninstrumented native samples estimate the
combined instrumentation effect; compare their distributions, not one pair.

Peak RSS is the child process lifetime high-water mark through rendering,
including imports and runtime initialization. Linux KiB and macOS bytes are
normalized to bytes. Other platforms report null, never zero. RSS is not engine
allocation size, a per-phase delta or the parent/child aggregate.

## Qualification and limits

Independent engine tests pin source-read/callback counts and stage order on
success, schema admission failure, cell access failure, output-key failure and
verification failure. Installed wheel and source-archive tests replay the
existing dataset and project-function corpora through instrumentation, retaining
the original expected values, CSV bytes, callback traces and interruption
assertions. Legacy no-binding entrypoints retain their own admission checks.
Timing totals must partition exactly, but no test asserts a machine speed.

These are small, cold-process fixtures. They do not qualify scaling, warm cache
behavior, R normalized-spec execution, production workloads or performance
regressions. Reference execution/verification remain combined in `run_total`;
per-value result conversion remains combined with native derivation. Separate
interpreter startup and phase-specific RSS are not measured. The named BMI slice
still needs the separately qualified POWER numerical policy. No tolerance change,
formula rewrite, speedup claim or completion of the whole step-6 gate follows
from this tool.

## Local characterization, 2026-10-05

Release build on macOS 26.6.2 arm64, Python 3.14.7, yamaa 0.2.0, native
0.1.0, Polars 1.44.2 and PyArrow 25.0.1. The build contains this measurement
slice on main `06d8e674b93a779e90bcf65c6b9d1a02611847fe`. Five fresh
processes per fixture/mode produced 60 exact CSV matches. No build or test job
was deliberately run alongside these samples; other desktop activity was not
controlled. These are local observations, not release performance budgets.

Median milliseconds (run includes ingestion through artifact construction):

| Fixture | Reference run | Native run | Profiled run | Reference process | Native process | Profiled process |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| adam-adlb-ordered-sum | 7.744 | 7.808 | 8.255 | 554.604 | 591.348 | 588.368 |
| schema-window-functions | 15.008 | 10.239 | 10.055 | 557.473 | 596.143 | 591.473 |
| schema-lookup | 6.290 | 7.850 | 7.895 | 551.099 | 582.970 | 585.496 |
| schema-functions | 31.744 | 33.490 | 33.375 | 579.417 | 617.171 | 616.796 |

Raw run-total milliseconds in repeat order, with no dropped samples:

| Fixture | Mode | Runs |
| --- | --- | --- |
| adam-adlb-ordered-sum | reference | 8.039, 7.744, 7.707, 7.734, 7.999 |
| adam-adlb-ordered-sum | native | 6.931, 8.962, 7.854, 7.808, 7.800 |
| adam-adlb-ordered-sum | native-profile | 8.803, 8.255, 7.939, 7.913, 8.822 |
| schema-window-functions | reference | 16.672, 14.948, 15.208, 15.008, 14.773 |
| schema-window-functions | native | 11.071, 11.118, 10.239, 10.063, 9.976 |
| schema-window-functions | native-profile | 10.055, 10.047, 9.663, 10.134, 11.173 |
| schema-lookup | reference | 7.325, 6.182, 6.290, 6.288, 6.386 |
| schema-lookup | native | 7.850, 7.740, 8.041, 7.786, 7.881 |
| schema-lookup | native-profile | 8.046, 7.294, 7.763, 8.954, 7.895 |
| schema-functions | reference | 32.494, 32.436, 31.428, 31.665, 31.744 |
| schema-functions | native | 33.054, 33.490, 33.931, 32.977, 33.935 |
| schema-functions | native-profile | 34.228, 33.375, 32.952, 32.419, 33.461 |

In profiled samples, package-import medians were 351–356 ms and schema loading
61–69 ms. The native call itself was 0.39–0.54 ms; derivation medians were
0.033–0.094 ms and verification 0.001–0.012 ms. Project activation in the
function fixture took 26.2 ms. Median peak RSS was 110.8–111.7 MiB for the
reference and 125.5–127.6 MiB for ordinary native execution.

Instrumentation deltas overlap ordinary sample variability here; they do not
establish a stable overhead percentage. Native execution has no consistent
end-to-end advantage in this cold, small-fixture sample. Imports, schema loading,
host planning/conversion and project activation deserve measurement as the shared
compiler replaces the temporary Python frontend. Broader scaling and warm-run
measurements remain required before choosing performance budgets or a cutover.
CI preserves a one-repeat smoke report per Python platform/version as a build
artifact; those smoke samples prove the harness runs, not statistically meaningful
performance.
