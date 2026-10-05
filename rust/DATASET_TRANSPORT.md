# Typed dataset bridge

`dataset/1` is an explicit temporary typed-plan bridge for #1585 steps 5 and 7.
It does not parse specifications, activate environments, select a backend, read
files or publish artifacts. Python remains default and every installation probe
still reports `execution_supported = false`. A future specification compiler must
admit the whole run and reject unsupported features before source/callback effects;
this transport rejects malformed typed plans rather than interpreting YAML.

Python calls `yamaa_native.execute_dataset(request, source)` with JSON text and
canonical IPC bytes, returning `(table_or_none, outcome_json)`. R calls
`yamaanative::execute_dataset(request, source)` with unclassed JSON character text
and raw IPC, returning `list(table, outcome)`. R uses the existing strict text
encoding policy and passes raw request bytes for checked UTF-8 decoding in Rust.
Both adapters invoke the same service synchronously on the calling thread.
The legacy entrypoints provide no callback authority. Explicit callbacks use the
separate entrypoint below; there are no retries, fallback or implicit external writes.

For multiple inputs, both hosts expose `execute_dataset_sources(request, source,
secondary)`. `secondary` is an ordered Python list of bytes or R list of raw
vectors. The plan adds optional `secondary: [{name, schema}]`, with unique names
and schemas in that same order. There are at most eight total sources. Aggregate
IPC bytes are at most 8 MiB, total decoded cells at most 262,144, and combined
source rows at most 65,536. Plan and buffer-count checks precede IPC decoding;
all snapshots are copied before execution. A legacy single-source call rejects a
plan requiring secondary snapshots. Existing single-source requests and outcome
bytes retain their previous shape.

## Request

See [the ADLB plan](crates/yamaa-adapters/tests/fixtures/datasets/adlb-plan.json)
for a complete independently authored request corresponding to the benchmark.
All objects reject unknown fields. The required top-level fields are:

- `protocol`: exactly `dataset/1`.
- `source` and `output`: ordered arrays of `{name, kind}`; kinds are `str`, `int`,
  `float`, `date`, `datetime`. Names are unique and nonempty.
- `templates`: ordered `{mode, assignments, filter?}` entries. Mode is `{records: null}` or
  `{groups: [source_column_indices]}` with nonempty, distinct grouping fields, or
  the sole `{keys: null}` template described below.
- `columns`: assignments for the later whole-column phase, in resolved order.
- `keys`: nonempty, distinct output column indices.
- `verifications`: ordered `{path, check}` entries with unique nonempty paths.

An assignment has an output `column` index, original diagnostic `path`, and one
`expression`: `{literal: scalar}`, `{source: index}`, `{column: index}`, or
`{reduce: {column: source_index, reducer: SUM_or_MEAN, text: original_expression}}`.
Indices are zero-based. Literal scalars use [the existing lossless codec](../R/yamaanative/README.md#scalar-transport-probe).
Reduction is only admitted in group scope over a numeric source column. Scalar
grouped reads must reference a grouping field. Output-column reads require an
already completed assignment. Every template completes the same row-phase fields;
remaining columns complete in the supplied order. Literal conversion occurs per
constructed value, including on plans reused with empty versus populated sources.
Paths and expression text provide provenance, not filesystem or artifact authority.

The additive `unconvertible` capability admits an optional top-level list:
`unconvertible: [{assignment_path, path, value: scalar}]`. Its order is declaration
order, independent of execution order. `assignment_path` must name an admitted
assignment; repeated assignments of a shared default must target the same output
column. Assignment references and handler paths are unique within the list, which
is limited to 1,088 declarations. Both paths retain authored provenance.

All declared paths register at zero before source execution, including empty
outputs and later unreached assignments. On initial conversion failure, one
literal replacement is counted and converted to the same target type. Explicit
missing is distinct from no declaration. Replacement failure is fatal at `path`
with complete keys when available; it cannot recurse or catch expression errors.
The scalar numeric lifecycle and dataset engine share this recovery function.
Only reached replacement text consumes scalar budget; a resource refusal before
replacement conversion leaves its count unchanged. Counts survive later resource,
conversion, output and verification failures and start fresh on plan reuse.

The additive `numeric_compute` capability admits `{compute: {text, bindings}}`.
Each binding is `{name, read: {source: index}}` or `{name, read: {column: index}}`.
Names must cover the compiled expression exactly, with no duplicates or unused
entries. Source reads are available in record scope and key construction; grouped
computations read completed outputs only. Non-key key-grain computations cannot
choose one feeding record and therefore also read completed outputs only.
The normalized-spec frontend additionally preserves the language's qualified-name
phase rules and refuses named-intermediate numeric bindings in this slice.

The existing Rust compiler preserves written association and evaluates each
identifier occurrence. Missing operands do not skip later operands, and literal
overflow remains deferred until reached. Arithmetic, ABS/MOD, numeric selection,
CEIL/FLOOR/TRUNC, SQRT and ROUND_HALF_AWAY_FROM_ZERO use the reference-compatible
subset. EXP/LN/POWER remain unsupported; this wire shape has no math-policy escape.
Compilation is bounded to 65,536 bytes, 8,192 tokens, 4,096 nodes/bindings/resolutions
and depth 64 before IPC decoding. Semantic node visits, cell reads and scalar
copies also consume cumulative runtime budgets.

Numeric validation failures carry no output identity; reached derivation failures
carry complete keys when available. Diagnostics reuse numeric/1 context and exact
source span/operand route. Ordinary result conversion follows evaluation, and
earlier source handler counts survive later arithmetic or conversion failures.
Empty input skips arithmetic and result conversion. Grammar failures and valid
unsupported functions are distinguished by the normalized-spec frontend before IO.

Numbering is represented by `{number: {kind, group_by, order_by, filter?}}`.
`kind` is `row_number`, `competition` or `dense`; `group_by` is an ordered list
of distinct completed output-column indices (empty means one global partition).
`order_by` is nonempty, with `{column, descending, nulls_first}` terms. Both booleans
are required. This expression is admitted only in key-grain column assignments;
source-qualified reads and row-phase windows are not represented.
Each window partitions completed output rows, sorts once per reached partition and assigns
its results back to their original positions. Null placement is independent of
direction. Integers retain full precision, finite floats sort numerically, temporal
precision does not affect ties, and construction order breaks remaining ties for
row numbering. Ranks compare only declared terms: competition leaves gaps while
dense rank counts distinct tuples. Empty outputs retain schema without conversion.
Fallible merge sorting charges each term comparison, text operands and each merge
pass against shared counters, stopping on the first resource failure. Numbering
results then pass through the ordinary completed-value conversion lifecycle.

The optional `filter` uses the bound predicate format below and requires the
additive `window_filter` capability. Only completed output bindings are admitted.
A partition evaluates every predicate in sorted order before producing its first
number; only true rows are numbered. False/unknown rows remain in the output and
receive missing. Ranks compare only eligible neighbors and number eligible positions.
Each partition is evaluated when its first output row is reached, before converting
that row's result; a later partition's predicate cannot overtake an earlier result's
conversion failure. Runtime predicate provenance is the owning window operation.
Empty output does not evaluate predicates against representative samples. All
predicate work/text shares the run's cumulative budget, including excluded rows.

Value reads use `{window: {kind, group_by, order_by, filter?}}` with the same
completed-output scope, ordering and eligibility rules. The `window_values`
capability admits three kinds: `{row_value: {column, offset}}`,
`{previous_non_missing: {column}}`, and `{locf: {column}}`. `column` names a completed
output donor column. `offset` is canonical signed-i64 decimal text and must be
nonzero. Offsets count only eligible positions; a missing or out-of-range donor
returns missing. Previous-non-missing skips any length of missing run and never
uses the current row; LOCF keeps the current present value, otherwise the prior
present donor. Excluded rows receive missing for every operation.

Each reached partition builds donor indices once, with a linear scan for prior
present values. Cached answers retain indices rather than cloned cells; the
current row copies its donor only when its own result is requested, charging text
before cloning and applying the ordinary conversion lifecycle. Full integer range,
float sign and temporal precision are retained. The legacy `number` form continues
to accept only numbering kinds; the `window` form accepts donor-value and baseline kinds.

The additive `window_baseline` capability admits
`{window: {kind: {baseline_flag: {date, reference_date}}, group_by, order_by: [], filter?}}`.
Both fields bind completed columns of the same temporal type (`date` or `datetime`).
Ordering is forbidden. After optional eligibility filtering in construction order,
a linear scan skips missing operands and compares each candidate date with that
row's reference date. The unique greatest qualifying date receives string `Y`;
other rows receive missing. Equal represented dates tie regardless of collected
precision. Multiple latest candidates raise `ambiguous_baseline` / REQ-0322 in
derivation phase, with exact count and canonical date text. Its condition outcome
has `identity: null` and `partition: [{name, value}, ...]` carrying declared grouping
keys (an empty list for global scope), separately from output identity. Partition
identity is budgeted before cloning. The current row materializes this condition
only if eligible; ordinary result conversion still applies to `Y`.

Checks include `{unique: [output_column_indices]}` and
`{row_count: {min: canonical_i64_text_or_null, max: canonical_i64_text_or_null}}`.
At least one row-count bound is required; `min` cannot exceed `max`. Missing bounds
may be omitted. Unique permits repeated references, matching the reference check.
Only error-severity, whole-artifact bounds are represented. The `source_selection`
capability supports ordered selection within `collect`. Broader source selection,
fractional bounds, grouped row counts, column checks, warnings, other handlers,
other windows and broader joins remain outside the closed plan vocabulary.
Typed function assignments are described under [Explicit host functions](#explicit-host-functions);
normalized-specification function lowering remains unsupported.

Predicate checks are `{assert: predicate}` or `{implies: {when: predicate,
then: predicate}}`, using the predicate representation below. Bindings may read
only completed output columns. Assertions fail on false or unknown; implications
fail when `when` is true and `then` is not true. Both sides are evaluated eagerly
per row. Each declaration first evaluates nonmissing representatives of all output
types, even for an empty dataset, with `when` validated before `then`. This detects
invalid predicate types before checking any actual rows. Checks run in declaration
order after all derivation, conversion and output-key checks.

The temporary compiler can also emit `{predicate_declaration: predicate}` as a
declaration-only checkpoint. It validates the representatives without evaluating
actual rows or emitting a verification record. This preserves a `when` type error
ahead of a pending `then` syntax/name error retained by the host compiler; it is
not an additional public verification operation.

## Standalone key combinations

A sole `{keys: null}` template represents absent/empty public `rows` (REQ-0042).
Its assignments complete exactly the declared output keys, in dependency order,
using direct source reads, earlier keys or literals. An optional `filter` is a
root predicate over source fields only; no output column is available yet.
The additive `root_filter` capability distinguishes this from row-template filters.
All source predicates finish in input order before any key conversion, preserving
a later predicate failure ahead of an earlier bad key. Only true source rows
feed key construction; false/unknown rows cannot contribute keys or collected
values. Retained memberships continue to point to the original source rows.
Every retained record completes all key conversions before non-key derivation.
Converted key combinations collapse in first-appearance order; each record with
a missing key remains separate for the later output gate. Names stay associated
with their values even when identity order differs from column/dependency order.

The later `columns` phase cannot use `{source: index}` in this mode. It instead
uses `{collect: {column: source_index, identifier: original_qualified_name}}`.
This reads every feeding record and counts distinct present raw values before
target conversion: no present value yields missing, one yields its first
representation, and multiple values yield REQ-0075 with the exact count and
complete output identity. For example, text `07` and `7` are two source values
even when an integer destination would convert both to 7. Repeated values and
missing readings do not create additional values. Collected reads are forbidden
in key assignments and in ordinary record/group templates. Multiple inputs,
other source handlers and additional expression families remain unsupported.

The additive `source_filter` capability permits an optional bound `filter` inside
`collect`. It reads source columns only and applies to the current combination's
feeding records, after any root filtering and key conversion. Every predicate for
that reading completes before any donor value is read. Only true records contribute
to distinct-value collection; false/unknown records are excluded without removing
the output row. No present eligible value returns missing, while multiple distinct
present values retain REQ-0075 and complete output identity. Runtime predicate
provenance names the owning source operation, not its `.filter` field. Source
filters remain forbidden in key, record and grouped assignments in this slice.

The additive `source_selection` capability adds optional `selection` within
`collect`: `{order_by: [{column, descending, nulls_first}], keep: "first" | "last"}`.
Order terms refer to source columns; the nonempty list has at most 64 terms.
All filters and distinct-value reads finish before selection. Only present donors
participate, and only multiple distinct raw values invoke ordering. The engine
finds the stable first/last extremum, comparing exact typed values and independent
null placement, with original source row position breaking ties. Equal source
values preserve their first representation without ordering.

An optional top-level `handler_counts` array follows `outcome` and is omitted when
empty, preserving earlier envelope bytes. Entries contain `spec_path`, `handler`
and canonical unsigned decimal `count` text. A completed ordered choice records
`multiple_matches` at the source operation path plus `.multiple_matches`, before
conversion. Counts remain on semantic, verification and resource-limit outcomes;
boundary/serialization errors still return no accepted envelope. Counts appear
in first-firing order, with no zero entry for an unused source choice. Prepared
plan reuse starts fresh budgets and counts. Source-filter and selection policies
compose within the same key-grain non-key reading.

## Secondary-source scalar reads

`multi_source` adds `{lookup: {source, column, keys}}` in key-grain non-key
assignments. `source` is the zero-based secondary index; `keys` is a nonempty
list of `{source_column, output_column}` pairs. Source key fields are unique,
outputs must already be completed, and paired logical types must match. This
initial lookup scans under cumulative work/text budgets; it is not a performance
qualification or a host-side index. Missing current or secondary keys never
match. The complete matching-record count precedes any donor value access.
No record returns missing, one returns its stored value, and multiple records
produce REQ-0127 regardless of value equality. This differs from base `collect`.

A multiple-match condition carries `matched_key: [{name, value}]` beside its
output `identity`. The diagnostic records source name, synthetic implicit
intermediate name and exact match count. The Python adapter reconstructs `key`
and `intermediate_key` context from the named typed values; output `keys` remains
the complete output identity. Cross-type key comparison and policies on implicit
secondary reads remain outside this slice.

## Named secondary intermediates

`named_intermediate` adds an optional request `intermediates` list, bounded to 64
declarations. Each item has `identifier`, `path`, secondary `source` index and
nonempty `keys` pairs with the same type/dependency rules as implicit lookup.
Optional `filter` binds only that secondary source, and optional `selection`
uses the same source-indexed `order_by`/`keep` representation as `collect`.
Optional `no_match` carries a tagged scalar: `{missing: null}` handles absence;
omission leaves absence as `unmatched_key` / REQ-0124. Assignment expression
`{intermediate: {index, column}}` reads a field from that named record selection.
Only key-grain non-key assignments can read it; all match outputs must be complete.

The first reached read materializes the whole source-only filter before matching,
including when current keys are missing or unmatched. Empty output and unused
declarations never evaluate the filter. Every matching record counts, including
identical or missing donor values. Multiple matches require ordered selection or
raise REQ-0127. Stable source position breaks ties, with exact typed order and
independent null placement. Selection is cached once per output row/declaration;
source eligibility is cached once per declaration within the attempt. Matching,
cache entries, order reads and scalar copies consume cumulative resource budgets.

Each reading records inherited `multiple_matches` or `no_match` at its own source
handler path before conversion. Cached reads repeat that accounting; matched
missing fields do not trigger `no_match`. Unused sites emit no zero counts. Join
conditions name the original intermediate path/id, include typed `matched_key`
and complete output identity. Predicate errors retain their filter provenance
without output identity. Every execution starts with fresh caches and counts.
Derived/SELF/base intermediates, correlated filters, ranges, extra handlers and
explicit row-template reads remain outside this bounded vocabulary.

## Row-template predicates

An omitted or null `filter` preserves earlier dataset/1 behavior. A filter is
`{path, text, nodes, root, bindings}`. `nodes` is a flat postorder arena; `root`
is its index. Bindings are unique `{name, read}` entries, with `read` either
`{source: index}` or `{column: index}`. Every identifier must be bound exactly once;
unused bindings are rejected. Column bindings must be completed in the row phase.
Grouped filters allow only completed output columns, including when a source field
is a grouping key. Every candidate completes row assignments and conversions before
filtering; only true survives, in original order, into the whole-column phase.
Missing/unknown and false both discard a candidate. Empty input evaluates no filters.

Scalar occurrences are `{literal: scalar}` or `{identifier: name}`. Node forms are:

- `{boolean: bool}`, `{not: child}`, `{and: [left, right]}`, `{or: [left, right]}`;
- `{compare: {operator, left, right}}`, with `eq`, `ne`, `lt`, `le`, `gt`, `ge`;
- `{is_null: {value, negated}}` and `{in: {value, items, negated}}`;
- `{between: {value, lower, upper, negated}}`;
- `{like: {value, pattern, escape, negated}}`, with null/omitted escape or one
  Unicode scalar. An empty IN list is invalid.

[Predicate semantics](PREDICATES.md) define eager occurrence order, promoted mixed
numeric comparison and Unicode LIKE. Text is diagnostic provenance; this bridge
does not parse it or invent syntax positions. Predicate conditions withhold output
and retain their original path, requirement and structural operand route, without
inventing output-key identity at the filter site.

Both host packages expose `dataset_capabilities()` as JSON text with
`protocol: "dataset/1"` and `features: ["row_filter", "predicate_checks", "key_grain", "window_numbering", "window_filter", "window_values", "window_baseline", "root_filter", "source_filter", "source_selection", "multi_source", "named_intermediate", "numeric_compute", "unconvertible", "row_source_lookup", "host_functions"]`. These additive capabilities are
separate from the unchanged full-backend readiness flag. The Python specification
frontend requires the corresponding feature before calling the source provider.
Older typed requests remain compatible when they omit these features.

## Outcomes and ownership

Plan admission precedes IPC decoding. The existing table preflight validates
framing, schema, shapes, buffer bounds, UTF-8 and logical text amplification;
owned Arrow arrays then supply immutable normalized cells. Output IPC is rebuilt
from visible owned values, preserving exact i64, binary64, missingness, ordering
and internal temporal precision. No source buffer is retained after the call.

Outcome JSON contains `protocol` and one `outcome`:

- `success`: accepted IPC plus all evaluated `verifications` in declaration order.
- `failure`: no IPC; `phase` is `output` or `verification`, with complete check
  observations including successful checks preceding/following failed checks.
- `condition`: no IPC; the existing scalar/numeric diagnostic encoding plus an
  optional `identity`. A failure has identity only after all output key fields
  have completed. A completed missing key remains distinct from an unavailable key.
  Predicate-check conditions additionally retain the completed `verifications`
  prefix, including earlier failed checks, and have no invented row identity.
- `limit`: no IPC; resource name and available decimal `limit`/`required` counts.

A check observation retains `spec_path`, condition/requirement, evaluated and
failed counts, output cardinality, and full offending identities. Each identity
has a decimal row `position` and ordered lossless `keys`. Counts never pass through
host binary64. Raw observations are intended for the caller and may contain input
data; they are not automatically logged or published. These are bridge observations,
not the complete public diagnostic/verification ledger, warning-log or sampling
contracts. Formatting those reports and retaining partial ledgers after boundary
resource failures remain integration gates.

Malformed requests, incompatible schemas, invalid IPC and boundary failures raise
host transport errors; semantic outcomes return normally. Neither host receives
accepted table bytes if evaluation, verification or response construction fails.
Control interruption is deferred while the bounded synchronous native call runs;
full workflow cancellation and asynchronous execution remain qualification gates.

## Resource policy

| Resource | Fixed limit |
| --- | --- |
| Plan JSON | 1 MiB UTF-8 |
| IPC input / accepted output | 8 MiB each |
| Outcome JSON | 8 MiB |
| Source/output columns | 64 each |
| Templates / verification declarations | 16 each |
| Assignments per template / column phase | 64 each |
| Schema name / provenance text | 256 / 1,024 UTF-8 bytes |
| Source/output rows | 65,536 each |
| Output cells / cells per partition key scan | 262,144 each |
| Cumulative logical work cells | 4,194,304 |
| Cumulative scalar input text processed | 16 MiB |
| Predicate nodes / bindings / IN items | 4,096 each |
| Predicate expression text / depth | 65,536 UTF-8 bytes / 64 |
| Cumulative predicate resolutions / LIKE work | 4,194,304 each |
| Retained output text | 1 MiB |
| Retained identity cells / identity text | 65,536 / 1 MiB |

Work cells charge assignments, direct/aggregate source reads, partition key scans,
key checks, row-count checks and identity copies before the relevant operation.
Scalar text accounting precedes cloning/conversion, so repeated large strings
cannot evade policy by converting to small numbers. Identity budgets apply before
copying keys and accumulate across checks and runtime failure context. Counters
reset per execution. Predicate node/scalar work shares the ordinary work counter,
and predicate text shares cumulative scalar text accounting. Predicate-check
representatives and actual rows consume these same cumulative budgets. Resolutions and LIKE
work have separate cumulative counters; each predicate also retains its smaller
per-evaluation limits from PREDICATES.md. Refusals identify `predicate_work`,
`predicate_resolutions`, `predicate_text_bytes` or `predicate_like_work`; required
counts are unavailable (`null`). Discarded candidates release retained output
text but never refund consumed work or operand text. Output row/cell capacity
bounds candidates before record/group template filters, not only final surviving
rows. Root filtering runs before any key-grain candidate exists; source cardinality
and key-width preflight limits still apply to the complete input.
Group comparisons still depend on key lengths and tree depth;
these policies are not CPU deadlines, allocator-byte guarantees or an OOM sandbox.
Input-byte and plan-complexity limits also bound work not counted as logical cells.
Key-grain construction admits source rows times key width against `key_cells`
before key reads. It stores only completed keys during probing; all temporary key
text consumes the retained-text budget until duplicate keys are released. Those
releases never refund work or processed text. Output row/cell capacity applies to
the resulting key combinations, so repeated input identities can collapse under
a smaller output-row limit. Collected reads charge every feeding cell and its
text before copying the selected value. Partition comparisons and sorting groups
by first source position remain subject to the documented non-deadline policy.

## Evidence and remaining gates

Shared fixtures encode committed ADLB source/expected values independently of the
Rust engine and include duplicate keys, failed row count, conversion timing,
empty input, temporal literals, integer overflow, and explicit true/false/unknown
row filters. Filter truth selects fixed row ordinals from the committed ADLB values. Rust, installed Python and
installed R replay the same observations; R needs neither Arrow nor a JSON package.
Predicate-check fixtures cover missing-value assertion identities, an eager
implication error after completed checks, and invalid types on empty output.
Key-grain fixtures independently pin converted identity/order, missing source
readings, empty output, raw-value conflicts before conversion and a valid key
shaped like the former Python missing-record token.
Installed tests check output-buffer independence, rejection before decoding,
post-error recovery and the unchanged backend capability flag.

The optional [Python normalized-specification frontend](../python/src/yamaa/adapters/NATIVE_DATASETS.md)
now composes this bridge with host loading/planning/IO and report formatting.
The shared Rust compiler and complete public reports/publication are not implemented
by this transport. The four named prototype datasets, all
benchmark cases, workflow/activation, numerical policy and release/default-cutover
gates remain tracked in #1585.

## Row-template secondary sources

`row_source_lookup` admits `{row_lookup: {source, column, keys}}`, with key entries
`{source_column, driver_column}`. Record scopes read raw driver fields; grouped
scopes require each driver field to belong to the grouping key. Key-grain plans
reject this form. Driver/secondary field types must match, target fields and key
references must exist, and secondary key fields cannot repeat. These rules are
checked before snapshot decoding.

The engine resolves driver match fields before scanning the secondary snapshot,
charges each read/text observation, skips missing keys and counts all matching
records before reading a donor. Zero records yield missing; duplicate records
retain exact raw match-key evidence even if donor values agree. The existing
completed-output lookup uses the same scan. Both run before assignment conversion.
Source errors remain opaque; budgets and diagnostic copy limits apply before reads
and copies. No index or performance claim is made.

Assignment placement controls execution order in direct typed plans and
`dataset/1` requests. A `row_lookup` in `templates[].assignments` executes before
that template's filter: even a false filter cannot hide its duplicate-match or
donor-read error. A `row_lookup` in `columns` executes after row filtering, only
for retained candidates; discarded candidates cause no lookup reads or lookup
errors. Retained candidates still use their original driver fields (grouping
fields in group mode), and duplicate matches still fail before donor reads.
The later result is unavailable to the earlier row filter. Key-grain plans reject
both placements. The normalized Python specification frontend currently lowers
secondary row reads only into template assignments; this typed-plan capability
does not expand its admitted specification vocabulary.

## Explicit host functions

Both hosts expose `execute_dataset_functions(request, source, secondary, callbacks)`.
The callback list is in declaration order and is captured for one synchronous run.
This is explicit caller authority to invoke those objects, not proof of artifact
membership or environment activation. The native entrypoints never discover code.

The optional top-level `functions` list contains at most 64 signatures with exactly
`identity`, `parameters`, `returns` and `may_return_missing`, using the signature
fields of [function/1](FUNCTION_TRANSPORT.md). A function assignment is:

```json
{"function":{"slot":0,"arguments":[
  {"name":"second","input":{"source":2}},
  {"name":"first","input":{"column":0}}
]}}
```

Each supplied argument has exactly `name` and `input`; inputs are `{literal: scalar}`,
`{source: index}` or `{column: index}`. Argument arrays retain authored order.
The shared signature rules supply defaults and map declaration-order host names.
All declared bindings and host names must be admitted before IPC decoding, even
on empty input or when all rows will be filtered out. Legacy entrypoints reject
function declarations without bindings before decoding any snapshot. Invalid slots,
unknown/duplicate arguments, omitted required names and illegal reads are rejected
at plan admission. Grouped source arguments must be grouping fields; key-grain calls
may occur only in non-key assignments over completed columns or literals.

The entire request remains bounded to 1 MiB. Each declaration and supplied argument
list permits at most 256 parameters; names use the function/1 limits. Copying a
signature into several call sites additionally consumes aggregate limits of 16,384
parameter entries and 1 MiB of identity/name/default-string bytes. These limits are
checked before each clone, so small declarations cannot create unbounded expanded
plans. Normal dataset work/text/output limits still apply. Callback wall time and
allocations inside arbitrary host code are not bounded by these logical counters.

Calls use the existing Python and R scalar ports, including full-range i64,
nonfinite/missing handling, temporal precision dropping at host argument encoding,
original interruption propagation and bounded error details. Python retains the
GIL; R callbacks stay on the R thread. Changing the caller's callback list during
a run cannot replace its captured slots. Nested or later runs receive fresh state.

Fatal invocation conditions include `spec_paths`, invocation context, known output
identity and declared handler counts; no accepted table is returned. Only conversion
of a successfully validated result can use an unconvertible handler. Boundary,
resource and internal errors retain their existing transport error classification.
No failure undoes earlier callback side effects or causes automatic retry.

Thirteen independently authored shared cases in `callbacks.json` / `callbacks.tsv`
pin complete output observations and traces for both installed hosts. Additional
tests cover metadata rejection before invalid IPC, expanded-plan limits, panic and
interruption containment, subsequent-run recovery, captured bindings, signed zero
and temporal encoding. This capability does not yet lower function expressions
from normalized specifications or implement production activation; the remaining gates
in issue #1585 and `execution_supported=false` remain unchanged.
