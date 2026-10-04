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
There are no callbacks, retries, fallback or external writes in this protocol.

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

Checks include `{unique: [output_column_indices]}` and
`{row_count: {min: canonical_i64_text_or_null, max: canonical_i64_text_or_null}}`.
At least one row-count bound is required; `min` cannot exceed `max`. Missing bounds
may be omitted. Unique permits repeated references, matching the reference check.
Only error-severity, whole-artifact bounds are represented. Root/source filters, fractions,
grouped row counts, column checks, warnings, handlers, other windows, joins, functions,
multiple sources remain outside the closed plan vocabulary.

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
using direct source reads, earlier keys or literals. It has no filter. Every
input record completes all key conversions before any non-key derivation runs.
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
in key assignments and in ordinary record/group templates. Root/source filters,
selection handlers, multiple inputs and additional expression families remain
unsupported.

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
`protocol: "dataset/1"` and `features: ["row_filter", "predicate_checks", "key_grain", "window_numbering", "window_filter"]`. These additive capabilities are
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
bounds candidates before each template's filter, not only final surviving rows.
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
