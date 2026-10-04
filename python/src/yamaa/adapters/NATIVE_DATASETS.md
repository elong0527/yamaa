# Optional native specification prototype

`native_datasets.execute_with_source_provider` accepts a `Specification` produced
by the existing schema loader and an explicit source provider. It returns a
`NativeDatasetRun` containing the ordinary `ExecutionSuccess`, `ExecutionFailure`
or `ExecutionUnsupported` result and all completed dataset `VerificationRecord`s.
It does not change `Domain`, CLI or conformance backend selection. Python remains
the default and the native installation flag remains `execution_supported=false`.

```python
from pathlib import Path
from yamaa.adapters.native_datasets import execute_with_source_provider
from yamaa.io import ProjectResources, load_source_tables, render_artifact
from yamaa.specification import load_specification

case = Path("benchmarks/adam-adlb-ordered-sum")
specification = load_specification(case / "spec.yaml", Path("yaml")).specification
resources = ProjectResources(case)
run = execute_with_source_provider(
    specification,
    lambda declarations: load_source_tables(declarations, resources),
)
if run.result.status == "success":
    csv_bytes = render_artifact(run.result.artifact)
```

Install both the ordinary Python package and the optional `yamaa-native` package.
The example renders bytes in memory; this API never publishes files. A caller
supplies approved source access through the existing `ProjectResources` port.
Arbitrary hand-constructed models that bypass schema/YAML validation are outside
this interface's input contract.

## Ownership and admitted semantics

The frontend rejects unimplemented syntax across the whole normalized specification
before requesting sources. It checks aggregate grammar rather than mistaking
malformed syntax for a valid unimplemented expression. The native entrypoint must
exist before the provider runs. Row-filter, assert/implies and key-grain requests also require the native
`dataset_capabilities()` advertisement before provider effects. Missing or
incompatible requested capability returns `ExecutionUnsupported`. Source-independent
filter scope/phase errors are also checked before IO, including grouped source
references and unavailable row columns; valid row-local defaults remain available
under REQ-1260. Binding against actual source schemas follows
source ingestion; Rust admits the complete bound request before IPC decoding.
The admitted specification is copied before provider effects, and the provider
receives separate source declarations so nested mutable model data cannot replace
the plan during IO. There is one provider invocation and one native dataset invocation, with no fallback,
reference evaluation, reference verification or callback execution.

The admitted subset is one source, explicit record/group row templates, direct
source or completed-column reads, scalar literals, and bare grouped `SUM`/`MEAN`
over one qualified numeric source column. Planner-resolved dependencies and
inherited row-template defaults retain their original phase/path. Conversion remains
per constructed value, so unconvertible literals do not fail empty templates.
All existing column types cross the table boundary without numeric narrowing.
Temporal precision remains inside native evaluation and drops only when accepted
values enter ordinary host table storage; ingested temporal values have the
reference's full day/second precision. Source ordinals are assigned by the existing
host ingestion port, then read as ordinary exact int64 values by Rust.

When `rows` is absent or empty, Rust evaluates and converts each retained input
record's keys, then constructs distinct combinations in first-appearance order.
Missing-key records remain separate for the output gate. Later direct source
reads collect all records feeding the combination: repeated present values count
once, missing readings do not add a value, and multiple distinct raw values fail
with their exact count and complete identity before target conversion. Keys may
depend on earlier keys; non-key dependencies and unsupported expressions retain
the existing planning/admission diagnostics. Key names remain associated with
their values regardless of identity order. This path requires `key_grain` native
capability before provider effects; it never calls the reference key constructor.

A non-key column can use `source: {variable: SRC.V, filter: predicate}` to narrow
its feeding records. `source_filter` is required before provider effects. The
predicate binds only qualified fields of the read's own source (REQ-0132), finishes
for all feeders before reading donor values, and retains the output row even when
none qualifies. False/unknown rows do not contribute values. Distinct-value counts,
conversion and diagnostics follow the existing collected-read lifecycle; runtime
predicate failures name the source operation and retain their primitive requirement.
Filters on keys, explicit record/group templates, ordered source choices and
source handlers remain outside this slice.

A root `filter` in this mode additionally requires `root_filter` before source
access. It reads qualified base-source fields only, retaining true rows and
omitting false/unknown rows before any key conversion. Every predicate evaluation
finishes before key construction starts. Collected non-key values use only the
retained feeding records. Root filtering and explicit rows remain incompatible
(REQ-1171); empty input skips predicate evaluation.

Explicit row-template filters support Boolean logic, comparisons, null tests,
IN, BETWEEN and Unicode LIKE, including negation and explicit ESCAPE. The existing
Python parser validates grammar and lowers the bound AST; Rust evaluates every
operand. Record filters can read source fields and completed row columns; grouped
filters can read only completed row columns. Filtering follows row derivations
and conversion, and precedes later whole-column derivations. Only true survives.
The public root/row-filter diagnostic projection preserves the reference wrapper's
omitted requirement; raw native diagnostics retain their requirement and operand
route. Out-of-i64 predicate literals and non-scalar Unicode text are explicitly
unsupported before source IO. Valid regex calls remain unsupported.

Rust owns output-key checks and error-severity `unique`, whole-artifact integer
`row_count`, `assert` and `implies` checks. Predicate checks use completed output
columns and the same admitted predicate families as filters. Rust validates
nonmissing type representatives even for empty output, then evaluates actual
rows; both implication sides evaluate eagerly. Check IDs are retained as report metadata. A later invalid
verification declaration stops the checks at that position, preserving earlier
records and overriding their data failures; derivation and key failures still take
precedence. The temporary compiler passes only that valid check prefix to Rust and
retains the pending declaration diagnostic. If an implication's `then` has a
syntax/name error, a native declaration-only checkpoint first validates `when`
without producing a record or evaluating actual rows. Thus an earlier `when`
type error wins, while a data-dependent `when` error cannot precede an invalid
`then` declaration. It never evaluates predicates or rechecks native data in Python.
Public diagnostic keys use the existing five-key sample, while private records
retain complete identities and exact counts. Requested verification logs survive
semantic failure, and no failed frontend result exposes an accepted artifact.

The existing host artifact adapter projects/orders accepted output and renders
CSV/Parquet and report bytes. Output ordering and decimals therefore remain explicit
host IO policies. Warning checks are not admitted, but a requested empty warning log
retains its existing header-only behavior. Metadata labels do not activate code.

`row_number` and `rank` (`competition` or `dense`) are admitted in the
key-grain column phase. Loader-expanded named windows and inline windows bind
only already completed output columns. Nonempty ordering supports ascending/
descending terms and explicit missing-first/missing-last placement; an empty
grouping list means one global partition. The native `window_numbering` capability
is required before the provider is called. Optional window filters additionally
require `window_filter`; they use the admitted scalar predicate vocabulary and
read completed outputs only. Native evaluation is eager and preserves every output
row, returning missing for false/unknown eligibility. Runtime conditions name the
owning window expression; empty output skips filter evaluation. Qualified source
reads and row-template windows remain explicitly unsupported.

`row_value`, `previous_non_missing` and `locf` use the same completed-output
window scope and require `window_values` before source access. Offsets are nonzero
signed integers and count eligible positions; neighbors with missing values stay
missing. Previous-non-missing and LOCF cross gaps, with LOCF retaining a present
current value. Native donor indexing is linear after sorting and copies values
only as their results are converted.

`baseline_flag` requires `window_baseline` and completed date/reference columns
of the same temporal type (`date` or `datetime`). Ordering is forbidden. Each
eligible candidate uses its own reference date; missing operands are skipped.
The unique latest qualifying candidate receives `Y`. A tied latest date raises
REQ-0322 with its exact count and declared partition keys, including missing keys
or the empty global identity. Filters use the existing `window_filter` gate.
Other comparable baseline types remain explicitly unsupported in this slice.

Ordered source selection, portable regex calls, additional expression operations, source handlers, column checks,
warning checks, grouped/filtered/fractional row-count checks, wide integer literals,
multiple sources, intermediates, producer schemas,
submission semantics, callbacks and environment/workflow execution are unsupported.
Unresolved inheritance remains unsupported; ordinary loader-resolved inheritance
uses the resulting normalized declarations. Shared Rust YAML resolution/compilation
will replace this temporary bridge at issue #1585 step 7.

## Limits and evidence

The [dataset/1 native policies](../../../../rust/DATASET_TRANSPORT.md) still apply.
Schema normalization, host source ingestion and IPC construction happen in Python
before native byte limits apply; this API is not an untrusted-input memory sandbox.
Native transport errors propagate as exceptions; `NativeDatasetLimitError` retains
the resource and exact limit/required strings separately from language diagnostics.
Boundary/resource failure does not currently expose a partial verification ledger.
No interrupted/failed run is retried automatically.

Installed wheel and rebuilt-source tests load the actual ADLB YAML/input and match
all 17 rows against the unchanged committed CSV, including exact ordered sums.
They separately compare real reference observations, failure diagnostics, complete
private keys and report bytes. During native execution, reference evaluator and
check functions are replaced with failing sentinels. Tests also cover validation
precedence/partial logs, temporal extrema/nulls, empty templates, known missing keys,
source ordinals, all admitted predicate AST families, eager filter failures,
fixed filtered ADLB CSV rows, assertion/implication truth and eager errors,
empty-output declaration validation, inherited defaults, projection/order/decimals, resource failure and
recovery. Source-independent tests prove unsupported features do not call providers.

Installed tests execute the complete unchanged 17-column `schema-window-functions`
specification against its committed expected CSV. Additional cases compare date
and datetime baselines, per-row reference dates, missing operands, three-way ties,
partition/global condition context, filtering, result conversion and empty output.
Numbering and donor cases retain directions/null placement and exact representations.

This qualifies the declared frontend slice only. The named lookup/BMI
prototypes, full compiler/workflow/activation/publication, R specification frontend,
all benchmark cases, performance measurements and release/default-cutover gates
remain open. No speedup or full language parity is claimed.
