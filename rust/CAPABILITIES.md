# Rust migration capability coverage

This matrix tracks the implemented slices of [#1585](https://github.com/elong0527/yamaa/issues/1585).
Unit and typed-plan tests are not installed specification execution. Every native installation probe still
reports `execution_supported = false`. The
[`execution-manifest.yaml`](../benchmarks/execution-manifest.yaml) still advertises
Python only; the explicit ADLB prototype below is not a qualified full backend.

| Contract / requirements | Rust core coverage | Python / Rust backend | R / Rust backend | Remaining gate |
| --- | --- | --- | --- | --- |
| Closed values: REQ-0002, 0006-0007, 0014 | Unit tests: distinct bool/column types, missing/absent selection, UTF-8 strings, full i64, finite binary64 wrapper | No dataset execution | No dataset execution | Host normalization, typed tables, source and function boundaries (#1585 steps 5-8) |
| Comparison: REQ-0004-0005, 0324 | Unit tests: present text, numeric and temporal values; exact mixed comparisons at large i64 values | No dataset execution | No dataset execution | Predicate grammar, operation-specific diagnostics, missing ordering policies (steps 4, 7, 9) |
| Basic arithmetic: REQ-0420-0421, 0424 (ABS/MOD only), 0427, 0430, 0434 | Shared hand-written vectors replayed by Rust primitives and Python evaluator; overflow, division, types, missingness, float bits | No dataset execution | No dataset execution | Host compiler and handler integration (steps 4, 7) |
| Typed numeric evaluation: REQ-0426-0427, 0429, 0438, 0443-0444 | Typed literal/identifier/unary/binary nodes, fake resolver port, ordered evaluation, exact failure provenance; 22 shared evaluation/trace vectors plus 49 arithmetic cases through the plan | No dataset execution | No dataset execution | Host diagnostic transport and lifecycle handlers (steps 4, 7) |
| Numeric syntax: REQ-0413-0415, 0439-0441 | Bounded arena parser; all 44 shared grammar cases, closed function/arity/reserved-word checks, diagnostic positions and deterministic Python comparisons | No dataset execution | No dataset execution | Full function execution and lifecycle diagnostics (steps 4, 7) |
| Numeric compilation: REQ-0413-0415, 0426-0427, 0434, 0438-0444 | Immutable supported-subset plans, deferred exact literal failures, source spans, unsupported preflight and static resolution budgets; 22 literal cases plus arithmetic/trace fixtures through compilation | No dataset execution | No dataset execution | Default math policy, host dispatch and full lifecycle (steps 4, 7) |
| Typed predicates: REQ-0159, 0166-0169, 0171-0177, 0189-0191 | Immutable bounded arena, three-valued truth, promoted mixed comparisons, null/IN/BETWEEN/Unicode LIKE and opaque resolver failures; 20 shared reference truth/trace cases and 7,225 independent LIKE comparisons | Root/row-template/source filters and assert/implies checks through normalized-spec frontend | Typed dataset/1 root/row/source filters and predicate checks | Shared syntax/regex, broader source selection and full installed integration; see [PREDICATES.md](PREDICATES.md) |
| Numeric selection: REQ-0415-0416, 0424-0427 | GREATEST, LEAST, NULLIF and COALESCE in primitives/typed evaluation/compilation; 51 shared value, promotion, missingness and eager-resolution cases | No dataset execution | No dataset execution | Host dispatch and full lifecycle (steps 4, 7) |
| Integral-valued numeric functions: REQ-0415, 0423, 0426-0427 | CEIL, FLOOR and TRUNC in primitives/typed evaluation/compilation; 100 shared cases with exact float bits, full-range i64, subnormals, signed zero and failures; standard-library differential tests across every finite exponent | No dataset execution | No dataset execution | Host dispatch and full lifecycle (steps 4, 7) |
| Square root: REQ-0422, 0426-0427, 0431 | SQRT primitive, typed evaluation and compilation; 32 shared exact-value/domain/failure cases and 110,188 bit-exact Python comparisons in native CI; pinned no_std libm | No dataset execution | No dataset execution | Default-policy EXP/LN/POWER and host dispatch (steps 4, 7) |
| Decimal rounding: REQ-0418, 0426-0427 | Exact bounded primitives, typed evaluation and both compiler policies; 34 shared vectors and 2,936 rational-oracle cases per policy | No dataset execution | No dataset execution | Host dispatch and full lifecycle (steps 4, 7) |
| Remaining math compatibility: EXP/LN/POWER | Candidate-only probe and per-platform JSON observations; exact differences block qualification, even at one ULP; default compiler still returns Unsupported | No dataset execution | No dataset execution | Choose and qualify explicit shared numerical behavior; include domain/zero/missing and dependent rounding/output contracts before enabling (steps 4, 11) |
| Temporal values: REQ-0539-0555, 0559-0561, 0567-0573 | Validated civil fields, strict parsing, canonical text, precision, equality and chronological order; 400-year calendar cycle | No dataset execution | No dataset execution | Imputation, temporal operations, function/artifact boundaries (steps 4, 5, 8, 9) |
| Conversion: REQ-0009-0013, 0015-0018, 0020-0021, 0601 | Full scalar matrix, strict numeric text, exact range/integrality checks, canonical numeric/temporal text, structured failures; shared reference vectors and deterministic float-text differential check | No dataset execution | No dataset execution | Host transport and integration beyond numeric derivations (steps 4, 7, 8) |
| Regex and remaining scalar grammars | Not implemented | No dataset execution | No dataset execution | Replay existing grammar/regex vectors, including backreferences and lookarounds (step 4) |
| Numeric completed-result lifecycle: REQ-0211-0214, 0343-0344, 0359, 0361, 0363-0364, 0366 | Engine service: conversion, optional literal replacement, structured fatal errors, deterministic per-path counts; 31 shared Python/Rust cases and fake-port reuse/failure/dependency tests | No dataset execution | No dataset execution | Normalized specification dispatch, host diagnostics and remaining handlers (steps 5, 7, 8) |
| Installed numeric application: REQ-0211-0214, 0359, 0361, 0366 and numeric conditions | Bounded numeric/1 protocol composes existing core compilation and engine lifecycle; 59 independent shared outcomes with typed diagnostic context, paths/spans, resolution order and handler counts | Installed wheel/source numeric execution; no dataset execution | Installed source numeric execution and lossless diagnostics; no dataset execution | Full specification dispatch, tables, callbacks, workflow and release qualification (steps 5-11) |
| Scalar host transport: REQ-0002, 0006-0007, 0014, 0570 | Adapter-only versioned JSON-to-core-to-JSON probe; 52 independent cases and byte limits | Installed wheel/source scalar round trips; no dataset execution | Installed source-package scalar round trips; full i64 carried as decimal text; no dataset execution | Arrow ownership, callbacks, diagnostic transport and full runtime integration (step 5) |
| Ordered table access and SUM/MEAN: REQ-0471, 0479-0480, 0487, 0492, 0510 | Borrowed normalized cells, ordered zero-row schema, bounded selected-column consumer; 32 shared exact outcomes plus fake-port error/selection tests | Reference aggregate truth replay; installed table transport, no dataset execution | Installed table transport, no dataset execution | Full aggregate compilation, installed reduction dispatch and lifecycle (steps 5-7) |
| Arrow snapshot storage: REQ-0002, 0006, 0479-0480, 0570 | Adapter-only owned batches implement TableAccess; closed schema, null normalization, exact values, per-cell temporal precision, chunk/slice/ownership tests and actual ordered reductions | Installed bounded IPC and Polars interchange; no dataset execution | Installed bounded raw IPC interchange; no dataset execution | R callback ownership and dataset dispatch (step 5) |
| Installed table interchange: REQ-0002, 0006-0007, 0570 | Bounded verified IPC, sanitized export and exact table/1 inspection; independent shared table truth and adversarial size/buffer/alias tests | Installed wheel/source plus non-editable Python facade; PyArrow/Polars exact logical round trips | Installed source raw IPC/inspection without Python or R Arrow; no i64 narrowing | Synchronous callbacks, specification/dataset execution and release qualification (steps 5-11) |
| Function invocation: REQ-0676-0686, 0700-0704 | Immutable signature/default validation; synchronous mapped callable port; fatal exact argument/result checks; 42 shared Python/Rust truth cases and ownership/effect tests | Installed wheel/source callbacks and reference BoundFunction replay | Installed source callbacks and 42 shared cases | Environment activation and dataset dispatch (steps 5, 6, 10) |
| Installed Python callbacks: REQ-0676-0686, 0700-0704 | Bounded function/1 request/result adapter composes the shared invocation service; strict admission and unwind/effect tests | Installed wheel/source real synchronous callbacks; all 42 shared invocation cases plus GIL/thread, ownership, reentrancy and exception tests | Installed source callbacks use the shared transport | Environment activation, dataset execution and release gates |
| Lossless R primitive scalars: REQ-0014, 0006, 0022-0027 | Validated tag/byte codec, shared checked arithmetic and exact numeric comparisons | Existing full-range native callback scalars | Installed int64/UTF-8 carriers, no NA collision/NUL loss, exact conversions and encoding/ownership tests | Connected to R FunctionPort; full workflow/release qualification remains |
| Installed R callbacks: REQ-0563-0564, 0570, 0676-0686, 0700-0704 | Shared function/1 invocation and checked temporal epoch codec | Installed callback facade preserves known temporal model precision | Real synchronous R callbacks; lossless scalars, explicit UTC, 42 shared cases plus ownership/reentrancy/interrupt/error/limit tests | Environment activation, specification/dataset execution and release gates |
| Dataset host callbacks: REQ-0570, 0676-0686, 0700-0704 | Typed function assignments, bounded signature expansion and fatal observations with explicit callback authority | Installed typed-plan callbacks, full-range scalars, control-flow and captured bindings | Installed typed-plan callbacks on the R thread, lossless scalars and original interruptions | Normalized-specification function lowering, production activation and full release gates |
| Python temporal result compatibility: REQ-0563, 0570, 0686, 0702 | Closed owned native temporal result carrier with exact field/calendar/precision admission | Optional non-editable facade bridges known DateValue/DateTimeValue subtypes; installed wheel/source independent truth and reference comparison | Existing Date/UTC POSIXct callback mapping | Full environment/workflow integration and release qualification |
| Typed single-source dataset application: REQ-0036-0039, 0042, 0044, 0047, 0059-0061, 0074-0075, 0211, 0240, 0381, 0385 | Immutable plan admission; record/group templates and standalone key combinations, direct/collected/literal/SUM/MEAN assignments, conversion, key checks and unique/row-count/assert/implies observations; committed ADLB source/expected replay plus failure/order tests | Installed typed-plan bridge; no specification compilation | Installed typed-plan bridge; no specification compilation | Full verification/report contracts, handlers, remaining operators and workflow/release gates |
| Bounded typed dataset bridge | dataset/1 composes immutable plan admission, owned Arrow snapshots and checked output; cumulative work/text/identity policies | Installed typed-plan entrypoint; optional Python normalization frontend below | Explicit installed raw-IPC/JSON entrypoint; no specification frontend/default dispatch | Shared Rust compiler, full portable reports/publication, remaining named fixtures and release gates |
| Normalized single-source specification bridge | Rust dataset/1 owns admitted derivation, conversion, grouping, output-key and dataset checks | Explicit optional Python loader/planner/IO bridge; actual installed ADLB YAML matches committed CSV and portable observations | Typed-plan execution only; no Python dependency introduced | Remaining named prototype fixtures, shared Rust compiler, workflow and release qualification |
| Full application/workflow, verification, publication | Not implemented | No dataset execution | No dataset execution | Fake-port application tests; Arrow and FFI tests; bounded vertical prototype (steps 5-10) |

`compare_present` mirrors the current Python value comparator: it does not round
an i64 through binary64 to decide ordering. Arithmetic promotion intentionally
does widen an integer when an operand is float. Boolean values are not accepted
by the ordered-value comparator, matching the existing Python implementation;
predicate truth/equality uses the separate typed `predicate` interface and required
mixed-number promotion. Missing placement
is likewise owned by the consuming operation, not silently chosen by this API.

Arithmetic primitives still take already-evaluated `Number` values. `NumericPlan`
now evaluates a caller-supplied typed tree for literals, identifiers, unary signs,
ABS, arithmetic operators and MOD. It resolves each occurrence left-to-right,
stops at the first failure, and evaluates both operands before missing
propagation. A missing left operand therefore does not suppress a failure in the
right subtree. Tree association is preserved and values are never memoized.

A core-owned resolver port returns normalized core values, absence, or its own
failure payload. Nonnumeric values produce REQ-0444 without implicit conversion;
absence produces REQ-0443 rather than missing. Resolver failures remain opaque
and intact, including data needed by later lifecycle handling. Numeric failures
retain condition, phase, requirement, exact arithmetic overflow, original text,
specification path and the structural operand route. The route is not a source
character span. No handler is selected, applied or counted by this evaluator.

This evaluator remains an internal typed IR, not a public host API. A separate
`numeric_parser` now accepts the full numeric syntax, preserves literal spelling
without converting it, and records half-open UTF-8 spans plus Unicode scalar error
offsets. Its immutable arena is protected by byte/token/node/depth budgets; depth
is capped at 64 even when a caller requests more. Resource exhaustion is separate
from grammar conditions. The parser handles every allowed function syntactically;
that does not claim execution support for those functions.

`numeric_compiler` now connects source text to immutable plans for the supported
arithmetic subset. It reports all valid unsupported calls in source order before
execution, retains runtime failure spans and enforces a static resolution budget
alongside the parser's structural limits. Every identifier occurrence counts,
including repeated names, even if an earlier failure would prevent reaching it.
The budget applies independently to each evaluation and does not bound time spent
inside a resolver. The caller-supplied `NumericPlan` API itself remains unbounded.
Default-policy EXP/LN/POWER, host diagnostic transport and lifecycle
integration remain outstanding.

Selection functions eagerly evaluate every argument before applying missingness,
selection and promotion. Variadic failures use zero-based argument routes and
retain the exact inner failure span. Extrema compare present values exactly before
promoting the selected result; NULLIF preserves the reference's comparison after
binary64 promotion for mixed pairs. All-int comparisons remain exact.

Python COALESCE previously returned the first present value without the REQ-0424
promotion required when any present argument is float. This slice explicitly fixes
that reference discrepancy alongside Rust support: the shared 51-case corpus pins
result types, float bits, resolver traces and downstream arithmetic. Missing
arguments do not force promotion. The correction may change large-integer results
or completed-result conversion and is not described as behavior-neutral parity.

CEIL/FLOOR/TRUNC always produce float for present input and canonicalize zero
results to positive zero, including negative-zero inputs and negative fractions
rounded to zero. Missing propagates after operand evaluation. The implementation
uses finite binary64 bits without a new dependency or bounded integer intermediate;
it preserves very large integral floats. The 100 shared vectors compare exact bits,
resolution traces and failure routes; Rust additionally compares standard-library
results at exponent/significand boundaries and deterministic finite bit patterns.

SQRT uses pinned libm 0.2.16 with default features disabled and an explicit dependency
allowlist entry. Negative finite input yields `sqrt_of_negative` (REQ-0431), while
negative zero remains a valid signed result. Integer input promotes before square
root; missing propagates. Source/operand order and static budgets use the existing
compiler. Exact Python math.sqrt comparisons run on each native Python CI target;
there is no blanket numerical tolerance. Other libm functions are not yet enabled
or claimed compatible, and no host/dataset backend is exposed by this slice.

Python numeric-expression integer literals now also avoid the host's integer
string digit limit: insignificant zeros are stripped before bounded construction,
and oversized positive literals report exact decimal text under REQ-0434. This is
a separate path from completed-result conversion. Overflow stays at evaluation
time in written order, before a literal's unary sign. Rust compilation now retains
that overflow as a deferred node instead of failing eagerly. Both implementations
replay 22 shared literal expectations, including signed i64 boundaries, 5,000-digit
inputs, decimal/exponent typing and nonfinite normalization. No integer literal is
narrowed through binary64.

Conversion failures retain the parsed source type/value used by Python, the
destination, phase, condition, eligible handler and owning requirement. An
out-of-range integer is retained as canonical decimal diagnostic text, never a
successful runtime value. Host error transport/serialization and handlers beyond numeric completed-result
conversion remain separate integration work. A decimal/exponent numeric spelling parses as
binary64 before integral conversion; plain integer spelling does not lose i64
precision by passing through a float.

Long integer text follows REQ-0015 without inheriting Python's configurable
integer-string digit limit. Both runners replay 105 shared conversion vectors,
including 5,000-digit inputs, leading zeros, signed i64 boundaries, exact large
integers, malformed text and overflow. Python strips insignificant zeros before
bounded integer construction. Diagnostics for more than 19 significant integer
digits retain canonical decimal text with source type `int`; shorter overflow
values keep their existing JSON numeric representation. This deliberate change
to oversized diagnostic values avoids unbounded integer construction and keeps
JSON/error-log serialization independent of the host's digit limit. Such values
never become successful runtime integers. Rust already retains all out-of-range
integer diagnostics as canonical text internally; its eventual host transport
must preserve this distinction.

The checked-in TSV contains explicit expected results, including raw binary64
bits (not digests). Both test runners compare to those expectations; neither
writes them. Temporal tests separately exercise strict lexical forms, leap
centuries, zone rejection, precision retention and loss on text round trips.

No new native API, default backend, benchmark manifest entry, or golden artifact
changes in this slice. Build qualification and the outstanding Cargo-lockfile
policy / R compiled-code warning are recorded in [`README.md`](README.md).

The candidate math assessment is not a new executable capability. On the local
macOS arm64/Python 3.14.7 sample, libm 0.2.16 differs in 989 EXP, 279 LN and 950
POWER results out of 10,011 inputs each (one ULP per observed difference). Native
CI retains complete per-platform observations as `math-assessment-*` artifacts.
Even a zero-mismatch sample is labeled `not-qualified`; default functions remain explicitly
unsupported; opt-in policy qualification is tracked separately below. Existing SQRT bit-exact checks remain strict and are not relaxed.


`MathPolicy::PortableLibmV1` now explicitly enables EXP/LN/POWER in the scalar
compiler using the pinned implementation. Default `compile_numeric` still returns
Unsupported for them. The 38 new shared semantic vectors retain exact written
truth; schema-2 reports compare all 30,033 candidate observations across the six
native Python CI targets and separately retain historical Python mismatches.
This is an opt-in implementation and sample-portability gate, not full accuracy,
legacy parity, host FFI or dataset qualification. See [MATH_POLICY.md](MATH_POLICY.md)
for migration implications and remaining release gates.

Decimal rounding is now implemented under both compiler policies. The independent
assessment in [ROUNDING_ASSESSMENT.md](ROUNDING_ASSESSMENT.md) identified and drove
correction of the pre-existing Python near-tie and overflow discrepancies.
Rust and Python replay 34 independent compute vectors, including eager failures;
Python also covers standalone and end-to-end conversion/CSV behavior. Native CI
requires both compiled Rust policies to match all 2,936 exact rational cases.
This qualifies the scalar slice, not host FFI, specification dispatch or dataset
execution. Numeric completed-result conversion and literal handler accounting now
compose these scalar results in `yamaa-engine`; host dispatch remains pending.

## Typed dataset application slice

`yamaa_engine::dataset::DatasetPlan` admits a complete, already bound single-source
plan before cell reads. Its closed expression vocabulary supports literal,
direct source, earlier completed-column, and grouped ordered SUM/MEAN values.
Record and group templates append in declaration order; groups retain first
source occurrence and member order. Grouped scalar source references must name
a grouping field, including in the later column phase. Every template completes
the same row-phase columns; remaining derivations complete whole columns in
resolved order. Each assignment converts before its dependents can read it. Literal conversion
also belongs to that per-value lifecycle: an empty record/group template has no
value to convert, while a populated template reports a runtime conversion failure
at `columns.<name>`. Paired Python reference and Rust regressions cover this timing;
plan admission must not eagerly reject an otherwise valid literal expression.

`table_grouping::partition` uses borrowed exact keys and a tree index, preserving
missing equality, signed-zero equality, civil temporal equality and exact UTF-8.
It does not sort records, hash content or round integer keys through binary64.
The snapshot port must provide correctly normalized cells of its declared types.
Source/output row, output-cell and key-cell budgets are resource limits rather
than language restrictions. These are not byte-level memory or allocator-failure
guarantees: strings, tree nodes and verification observations allocate normally.

Absent/empty row templates can use native standalone key construction: every
input key lifecycle completes before non-key derivation; converted combinations
retain first occurrence and complete feeding memberships. Missing-key records
remain separate. Direct source reads count all distinct present raw values before
conversion, preserving exact counts and identity on REQ-0075 conflicts. Named
identity and dependency order remain separate; unhandled conflicts never select an arbitrary source representative.

Root predicates (REQ-1170) now narrow key-grain input before key construction.
They bind source fields only; every predicate completes before any key conversion.
Retained memberships preserve source row coordinates, and collected values cannot
read excluded records. Installed comparisons cover eager predicate failures,
conversion priority, false/unknown exclusion and empty input; shared raw cases
replay through Rust/Python/R. Root filtering requires its own capability.

Collected non-key source reads can narrow feeding records with a source-only
predicate (REQ-0131/0132). All eligibility checks for a reading finish before donor
access. Excluded records cannot introduce distinct values, and missing selections
retain their output row. Source predicates execute after all key conversions;
root and source filters therefore preserve different failure order. Shared fixtures
and installed comparisons cover missing selections, conflicts, conversion, errors
and root-filter intersection.

Paired source `order_by`/`keep` can select the first/last present donor for a
conflicting non-key reading (REQ-0354). Exact typed order, independent null
placement and stable source ties execute in Rust after eligibility and distinct
collection. Equal values bypass order reads and retain their first representation.
Run-local `multiple_matches` counts survive later conversion/check/resource failures,
reach both installed adapters, and reset on plan reuse. This bounded selection
requires its own capability. Other source handlers and broader source scopes
remain unsupported.

Multi-source key-grain execution now accepts independently owned secondary
snapshots and bound many-to-one reads on completed output keys. Same-type
matching excludes missing keys, counts all records before donor access, and
retains REQ-0127 match-key context and output identity. No match gives missing;
identical duplicate values remain multiple matches. Shared fixtures cover values,
absence, empty base, duplicates and conversion; installed comparisons prohibit
host lookup dispatch. Total bytes/cells/rows and cumulative scan work are bounded.
This initial scan is not a performance claim; mixed numeric key matching remains
open. Named secondary intermediates additionally cache source-only eligibility
once per reached declaration and selected records per output row. Same-type
match keys may read earlier completed outputs; ordered choices count records,
and literal absence handling preserves REQ-0124 when omitted. Each reading
records inherited handlers at its own source path before conversion. Shared
fixtures and installed tests cover exact values, counts and failure priority;
the complete unchanged `schema-lookup` benchmark reaches its committed CSV.
Derived/SELF/base intermediates, correlated filters, range matching and explicit
row-template reads remain open, as do shared compilation and release qualification.

Dataset `row_source_lookup` adds bounded record/group-template secondary reads on
raw driver match keys. It shares the existing duplicate-count/absence scan and
keeps match keys separate from converted output identities. Six independent
shared fixtures and installed record/group/input-order/failure variants accompany
the complete unchanged 24-row ADVS BMI benchmark. Multiple row drivers, broader
joins and the separate ADSL BMI function/POWER integration remain open.
Direct typed plans can also place raw-driver lookups in the later column phase,
where only retained candidates are read. Duplicate-before-filter ordering applies
to template assignments. The normalized frontend admits only that template form;
see [the phase contract](DATASET_TRANSPORT.md#row-template-secondary-sources).

Dataset `unconvertible` composes the same completed-value recovery across every
admitted assignment, including keys, row/group columns, windows and named reads.
Explicit declarations register at zero in planned order before execution; only
failed result conversion counts and converts a literal replacement. Replacement
failure retains its own path, while arithmetic/selection failures bypass handling.
Seven independently authored shared cases and installed reference comparisons
cover replacement values/missing/failure, empty runs, dependent reads, filters,
key grouping, arithmetic bypass and inherited selection counts. Other handlers
and broad language/release qualification remain open.

Dataset `numeric_compute` composes the existing compiler with statically bound
source/completed-output reads. Each written occurrence is resolved without
reassociation; deferred literal overflow, eager missing operands, exact numeric
conditions and current-result conversion retain ordinary dataset failure order.
The shared six-case fixture adds typed values, requirements, identities and source
geometry; installed comparisons cover 162 arithmetic/type combinations and 48
failure/row-source/conversion variants with Python numeric evaluation blocked.
Cumulative semantic-node/read/text budgets apply after bounded pre-IPC compilation.
EXP/LN/POWER, expression-local handlers and named-intermediate numeric bindings remain open;
the separate portable-math candidate does not change this dataset policy.

The key-grain column phase also supports row numbering and competition/
dense rank over completed output columns (REQ-0293/0301/0303/0340). Partitioning,
exact typed ordering, explicit null placement and stable construction-order tie
breaks execute in Rust, with cumulative comparison/text/merge work limits. The
installed Python tests execute the unchanged `SEVRANKC` and `SEVRANKD` declarations
and their eight input-derived columns from `schema-window-functions` against the
projected committed expected CSV. The same test now includes the unchanged filtered
VSSEQ declaration (REQ-0294). Only true rows are numbered; false/unknown rows retain
their output position with a missing value. A reached partition completes predicate
evaluation before converting the current result; later partitions remain lazy.
Row-value reads, previous-non-missing and LOCF now share that scope (REQ-0061/0294/
0328/0340). Donor indices are cached once per reached partition; only the requested
result is copied and converted, retaining integer/temporal/float representations.
Temporal baseline selection now uses each eligible row's reference date, skips
missing operands and selects the unique latest candidate (REQ-0322/0341).
Date and datetime columns must match; other types remain unsupported. Ties retain
exact counts, canonical date text and declared partition keys, distinct from output
identity. The installed test executes the complete unchanged 17-column
`schema-window-functions` benchmark against its committed CSV. Additional installed
cases compare filtering, missingness, per-row references, three-way ambiguity,
global partitions and conversion failures. This qualifies that named window case,
not the remaining language, workflow or release gates.

Output keys are checked before dataset verifications. The closed error-severity
verification subset includes unique combinations, whole-artifact integer
row-count bounds and assert/implies predicates on completed output columns.
Predicate declarations validate nonmissing type representatives even for empty
output; implications evaluate both sides eagerly. A predicate condition retains
the completed ledger prefix and stops later checks. Failed output identity discards the table; failed dataset
verification retains all evaluated records in declaration order, including
successful checks. Observations retain actual artifact cardinality and complete
output-key identities for later report formatting. This is not yet the public
verification ledger/diagnostic schema, sampling policy or publication workflow.

The Rust application test reads the committed `adam-adlb-ordered-sum` source and
expected CSV, supplies an explicit typed plan, and checks all 17 projected rows,
column types, missing values and check counts. The fixture's `0.6000000000000001`
and `0.6` totals remain distinct. This proves dataset application composition,
not YAML compilation, installed host dispatch, source discovery or release
qualification. No capability flag or benchmark execution manifest changes.

A future compiler must reject the entire run before execution if it requires
unsupported syntax: other handlers, normalized-specification callbacks, broader source selection, regex, other windows, broader joins, other expression operations, column checks,
warning checks, grouped/filtered/fractional row counts or file publication.
The temporary typed-plan bridge is tracked by #1585 steps 5 and 7; the full
Rust specification compiler and the remaining benchmark gates are still open.

The optional [dataset/1 bridge](DATASET_TRANSPORT.md) connects this application
service to copied canonical IPC. It bounds plan complexity, scalar text processed,
retained output/identity payloads and cumulative logical work before exposing the
service to host requests. Runtime failures retain known complete output identities;
unconstructed keys remain unavailable. The bridge reports raw typed observations,
not complete public verification logs, warning handling or publication. An optional [Python normalization frontend](../python/src/yamaa/adapters/NATIVE_DATASETS.md)
now executes the ADLB specification through this bridge. The shared Rust compiler,
remaining named prototypes and all full-workflow/default-cutover gates remain open.

The trusted Rust dataset API also composes prebound host-function calls with
record/group assignments and completed key-grain outputs. The explicit Python/R
`execute_dataset_functions` bridge checks bindings before IPC decoding and reuses
the same scalar invocation lifecycle. Nineteen shared cases and host boundary tests
qualify this typed-plan subset; normalized-specification function dispatch and
production activation remain unsupported.
See [dataset function composition](DATASET_FUNCTIONS.md) for phase, error and
resource contracts and the remaining frontend/activation qualification gates.

The additive `function_source_collection` capability admits plain primary-source
arguments for key-grain non-key callbacks. It reuses distinct-present collection,
authored resolution order, source diagnostics and cumulative budgets. This is a
prerequisite for the unchanged `schema-functions` specification; normalized call
lowering and native project activation are still unqualified.
