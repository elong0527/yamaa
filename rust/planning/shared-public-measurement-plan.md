# Shared public measurement plan

This is the implementation plan for B17/B18 in the
[release blocker register](release-readiness.md), assessed against main
`65249a577d1cbfafd4f59c7d081a4636c8aed119` on 2026-10-11. The maintainer approved
the scalable workload sizes, sampling and baseline below. Numeric budgets and
release transition remain undecided.
The first-release support matrix and exclusions are already approved in the
[maintainer decisions](maintainer-decisions.md#first-release-support-matrix).

The [existing phase harness](../PHASE_MEASUREMENTS.md) measures Python's older
host-normalized dataset route. Keep its historical observations intact. The new
[opt-in collector](../tools/measure_public_lifecycle.py) measures ordinary installed
`yamaa.domain(...)` and `yamaa_domain(...)`, copied result views and explicit held
`save()` independently.
Optional instrumentation must delegate to those same implementations.

## Bounded collector and replay

The implemented [worker protocol](../tools/public_lifecycle_worker_protocol.json)
selects three unchanged cases: ordered-sum, window-functions and lookup. A
correctness smoke uses one cold child and one warm child for every host/case;
the warm child retains one declared warm-up and one measured repetition. This
creates twelve children and eighteen public Builds, each with a new Domain and
fresh directory, followed by two Saves of the held result. A smoke establishes
collector execution and exact truth, not a performance distribution or B17/B18
completion. The scalable extension below now implements independent input/truth
generation and uses these same installed workers.

For an already qualified installed Python wheel/source and R source package,
provide separate installation-provenance JSON files and absolute paths:

```sh
python3 /path/to/yamaa/rust/tools/measure_public_lifecycle.py \
  --repository /path/to/unchanged/source-checkout \
  --revision <complete-source-commit> \
  --python-provenance /outside/python-installation.json \
  --r-provenance /outside/r-installation.json \
  --python-executable /outside/installed-venv/bin/python \
  --r-executable /path/to/Rscript \
  --output /outside/fresh-lifecycle-evidence \
  --purpose correctness-smoke --repeats 1 --warmups 1 --compilation-clear
```

Each provenance file declares the exact source commit, artifact reference/path,
package form, runtime/package/core versions, actual package/native origins,
system, build-attested toolchain and CPU or explicit null, and a retained
installation record tying those facts together. The controller directly compares
a supplied wheel's native bytes or a supplied source archive's original native
sources, manifests, lock and toolchain. Source-build binary association remains
an installation/build attestation. The executed harness bytes are retained;
its surrounding checkout head is separate from the installed source identity.
The output directory must be fresh and outside every checkout.
The worker launches the supplied absolute Python executable spelling so that a
virtual environment retains its `pyvenv.cfg` context. Its real executable target
is independently audited; both invocation and loaded origins must be outside
every checkout.

R additionally requires explicitly located `jsonlite` and `microbenchmark`
1.5.0 collectors, their actual versions/origins, the selected timer build config,
and byte-identical built/installed timer libraries. A reviewed monotonic provider
is required; the wall-clock fallback refuses. Collector import and request
JSON decoding precede the timed `yamaa` import. The worker uses raw nanosecond
timestamps without a timing wrapper, implicit warm-ups or overhead adjustment.
R's binary64 timestamps are transported as 17-significant-digit decimal strings
and exact IEEE bytes. The controller requires their round-trip identity and
exact binary64 subtraction. Python timestamps remain integer nanoseconds.

Import is timed once per child. Public Build is the first explicit native call;
metadata follows Build. Copied output/issues/logs include their actual host
transport. Each held Save has its own timer. Raw report and saved-byte retention
and controller comparison occur outside these boundaries. Complete Build and
both Save reports, typed copied values/order/float bits, issue rows, null logs,
both original CSV comparisons and repeated held bytes must pass before any
sample is accepted. Every warm-up and failure remains visible.

Phase attribution, actual callback counts, process startup, shipped-schema
initialization and filesystem-cache observations are currently null. Python
Linux/macOS peak RSS uses child-lifetime `ru_maxrss` with original KiB/bytes;
it is cumulative across warm repetitions. R and Windows peak RSS remain null.
Characterization is separately opt-in and does not supply an approved budget.

Keep all compilation stopped through collection. The explicit
`--compilation-clear` attestation is supplemented by a process-name inventory
before every child; an unavailable probe or active compiler refuses that slot.
The inventory is not an atomic global lock. The first retained local attempt
correctly refused all twelve slots when a Cargo process appeared, before any
candidate launch or sample. Replays must use a fresh directory and a new clear
interval; failed attempts are retained. Independent synthetic-owner and actual
R-worker controls cover reached-report preservation, original failures and
interruptions, secondary transport failures and owner release without retries.
The second retained attempt launched all twelve children but remained rejected:
Python lost its virtual-environment invocation context and ordinary R JSON
numeric printing lost timestamp precision. A separate read-only audit of the
reached R observations passes nine repetitions, 27 complete reports and eighteen
original CSV comparisons; that functional evidence does not accept the rejected
collection or reconstruct its clocks. The third fresh attempt, with corrected
launches and lossless transport, passes independent read-only replay of all twelve
actual children, eighteen Builds including six warm-ups, 54 complete reports,
36 exact CSV comparisons and 156 unadjusted raw clock boundaries. All 51 retained
original files and the four executed harness files compare byte-for-byte with
their immutable source and reviewed snapshot. This observation covers installed
`ad34de8d75fbbcc4392d42b93c0fa86745823788` Python wheel and R source packages on
local macOS arm64, CPython 3.14.0 and R 4.6.1. Harness regression discovery passes
123 tests, including actual symlink launch, authored binary64 transport and
owner-failure controls. It establishes this bounded collector's correctness;
characterization and B17/B18 remain open.

## Workloads and truth

Start with unchanged ordered-sum, window-functions and lookup CSV goldens, the
original `schema-functions` YAML/locks/CSV, and the eight immutable producer
cohort files. Require complete original public observations and saved bytes
before collecting timing samples. Verify installed Python module and R library
origins are outside the checkout; retain the exact source revision, installed
artifact reference, package form, toolchain, runtime, system and CPU. Compare
source and fixture bytes directly, without content hashes.

| Workload | Independently varied dimensions | Required independent observations |
| --- | --- | --- |
| Ordered aggregate | Driving rows, rows per group, group count, cancellation and missingness | Source-order values, group identity, types/order and exact artifacts. |
| Windows | Partition sizes/counts, ties, missing runs and offset distances | Rank/offset/previous-non-missing/locf/baseline values and stable order. |
| Lookup | Base keys, donor multiplicity/ties, unmatched keys and correlated selections | Complete values/order, stable first/last selection and donor/handler counts. |
| Host callbacks | Missing short circuits, call count, types and argument width | Actual delegated calls and argument/result traces, per-build lock/binding/tests and original errors/interrupts. |
| Producer workflow | Chain depth, fan-out, shared nodes and consumer sizes | Producer once, activation before study reads, serialized rounding, complete node/source/check ledgers and both held Saves. |

The maintainer approved 1,000/10,000/100,000 driving rows, ten cold samples and
thirty warm samples after three warm-ups, comparing a compatible previous
release on the same machine. Numeric regression budgets follow review of those
measurements. This approval does not change runtime quotas or promise capacity.
Keep original fixtures unchanged and retain refusals and reached stages.

The candidate-free [workload generator](../tools/prepare_public_workloads.py)
authors 27 variants: lookup donor multiplicity 1/4, aggregate group width 10/100,
window partition width 10/100, callback missing short circuits, and producer
shared chain depth 1/3 with two consumers. Independent algorithms pin first-tie
selection, exact source-order floating-point folds, missing-aware window values,
typed root cells and all serialized producer/root bytes. Existing complete host
locks are copied unchanged. Expected callback counts are explicitly independent
expectations; actual counts stay unavailable until observed.

```sh
python3 rust/tools/prepare_public_workloads.py --output /outside/fresh-workloads
python3 rust/tools/measure_scalable_public.py \
  --repository /path/to/source-checkout --workloads /outside/fresh-workloads \
  --case aggregate-1000-10 --label candidate --purpose characterization \
  --python-provenance /outside/python-installation.json \
  --r-provenance /outside/r-installation.json \
  --python-executable /outside/installed-venv/bin/python \
  --r-executable /path/to/Rscript --output /outside/fresh-evidence \
  --compilation-clear
```

Run separately for each case and candidate/baseline label. The controller
regenerates the declared case and compares every authored member and the
retained generator bytes before launching any host. Every repeat has a fresh
directory. Admission requires independent typed output, node/source ledgers,
zero actual published files after Build, successful repeated Saves and exact
named artifact identities/order/columns/row counts/bytes. Some installed owners
expose prospective artifact records during Build; those records are retained
and checked against completed Save observations without equating them with
disk publication. The full raw reports remain available for inspection.
No failing repetition becomes a sample or is retried. A correctness smoke uses
`--purpose correctness-smoke --cold 1 --warm 1 --warmups 1`; its durations never
produce a characterization distribution. Phase/callback counts and unavailable
memory metrics remain null.

Pinned Rust/Cargo 1.90.0 current-source functional controls retained nine variants
at 20 rows with exact typed output and both Saves for Python and R on local
macOS arm64, CPython 3.14.0 and R 4.6.1. These are workload/worker controls,
not qualified release timings. A separate Python frontier retained all 36
Build/Save attempts at 20/1,000/10,000/100,000 rows: twelve pass (all nine small
cases, both 1,000-row aggregates, and the 1,000-row callback), while 24 refuse.
At 1,000 rows lookup/window report publication or result projection limits and
producer report limits prevent acceptance. Every 10,000/100,000-row case refuses;
100,000 rows also reaches source/original-boundary limits. Raw evidence is retained
under `/private/tmp/yamaa-measurement-pinned-frontier`,
`/private/tmp/yamaa-scalable-python-functional-v2` and
`/private/tmp/yamaa-scalable-r-functional-v2` on the collecting machine.
The maintainer capacity decision is pending. B17/B18 stay open until the approved
sizes can be measured, qualified candidate/baseline evidence is retained, and
numeric budgets and remaining metrics are approved.

## Cold, warm and reuse protocols

Cold samples start a fresh Python or R process and measure its first public
Build after import. Use an R script file, preserving the established installed
R launcher. Record process startup/import and the first metadata/native call
separately. A pre-timing `engine_info()` must not warm native initialization
silently. Record shipped-schema initialization and operating-system filesystem
caches as uncontrolled or deliberately managed.

Warm samples repeat new public Builds in an imported process. Every repetition
creates a new Domain and freshly verifies locks, bindings and declared tests;
there is no prepared-model or activation cache. Repeated Save of one held result
is a separate reuse experiment. Use fresh work directories to keep prospective
output-presence state consistent.

Retain every repetition, predeclared warm-up and execution order. Rotate ordinary
and instrumented sample order and report spread/quantiles alongside medians.
Run characterization without concurrent package compilation. One-repeat CI
samples establish that the harness executes, not a performance distribution.

## Phase attribution

Thread an optional clock-free stage observer through actual shared file
preparation, project activation, execution and result transport. Adapter-owned
monotonic clocks may measure the following work only where it is observed:

| Boundary | Included work and limits |
| --- | --- |
| Preparation/compilation | Original source capture, inheritance/schema preparation and shared compilation. |
| Environment activation | Fresh lock verification, callable binding and all declared tests before study data. |
| Ingestion/admission | Source capture/decoding, input admission and actual engine admission. |
| Derivation/output/verification | Actual execution and existing engine stage transitions. Per-value callback/result conversion stays in derivation until separately measured. |
| Result conversion | Complete report/artifact encoding and copied host views. |
| Save | Held-byte publication, with producer order and per-node observations separate from Build. |

The existing dataset observer is not a complete public lifecycle partition.
Producer attribution needs per-node observations and graph totals. Record nested
components as nested; never add them again to an outer Build duration. Omit
unreached stages, retain original failures and propagate interruptions. Observers
must not retry callbacks, reread sources or publish to fill a metric.

Count actual delegated host calls, rather than selected function names, handler
occurrences or an empty report `callbacks` array. Keep lock/binding/test activity
separate from derivation callbacks. A once-delegating observation must preserve
original exceptions and argument/result order. Match ordinary and instrumented
complete reports, saved bytes, source counts and callback traces before
interpreting instrumentation overhead. Keep zero-write/refusal, interruption and
both-Save controls as correctness evidence outside speed comparisons.

## Memory and budget decision

Record the child process lifetime peak RSS with its provider and original units.
Python's existing Linux KiB/macOS bytes normalization is usable when explicitly
labelled. R needs a validated child peak provider or null/unavailable; Windows
also remains unavailable until a provider is validated. Parent RSS, allocator or
GC statistics are not child RSS. A lifetime peak is not a phase delta, and warm
lifetime high-water RSS is cumulative across repetitions. Missing metrics are
null, never zero.

B18 needs a maintainer decision on representative workloads/sizes, repetition
protocol, machine classes, cold/warm/phase and memory metrics, allowed regressions,
comparison baseline and acceptable unavailable metrics on supported platforms.
The historical tiny-fixture results supply no numeric threshold or default
transition approval. B17 closes only after retained shared-path measurements meet
the agreed evidence scope; B18 closes only with the approved numeric decision.
