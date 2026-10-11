# Shared public measurement plan

This is the implementation plan for B17/B18 in the
[release blocker register](release-readiness.md), assessed against main
`8e97023fe93a1d40a762be35c286bbbd5695137d` on 2026-10-11. It proposes evidence
collection, not approved workload sizes, numeric budgets or release transition.
The first-release support matrix and exclusions are already approved in the
[maintainer decisions](maintainer-decisions.md#first-release-support-matrix).

The [existing phase harness](../PHASE_MEASUREMENTS.md) measures Python's older
host-normalized dataset route. Keep its historical observations intact. A new
opt-in harness must first measure ordinary installed `yamaa.domain(...)` and
`yamaa_domain(...)`, copied result views and explicit held `save()` independently.
Optional instrumentation must delegate to those same implementations.

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

The suggested 1,000/10,000/100,000 driving rows are proposals, not approved
sizes, budgets or capacity promises. Author each scalable input/expected-output
manifest and a separate pure expected-value/byte algorithm before running the
candidate. Keep original fixtures unchanged. Retain actual capacity refusals and
reached stages; do not raise runtime quotas to obtain a timing sample. Follow the
approved numerical policy with exact comparison, without tolerance or formula
changes to make a sample pass.

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
