# Windows

A window expression reads other rows of its partition: it numbers or ranks
them, reads a neighboring row, carries a value forward, or flags a baseline
row. Its `window` declares the partition, the order and a row filter, written
inline or named in root `windows`.

## Named windows

<a id="window-1"></a>

### WINDOW-1. A window reference names one complete definition declared in root windows

Root `windows` declares complete window settings by identifier. An
expression's `window` accepts either an inline `window_spec` mapping or one of
those identifiers as a string, for example `window: RESPONSE_ORDER`. A named
definition is an inline mapping, never another reference. A reference accepts
no additions or overrides, and there is no `ref` mapping form. A surviving
reference to an undeclared window fails.

Fails: `unknown_window`.
Evidence: `negative-unknown-window`, `schema-window-functions`, `adam-adae-severity-rank`.
Replaces: REQ-1251, REQ-1253.

Omitted fields keep their ordinary window meanings: omission does not create a
parameter the caller can supply. Names belong to a separate window naming list
and are compared exactly.

An undeclared window fails validation as `unknown_window` at the expression's
`window` field, with the unknown name in context `window`. Malformed
definitions, reference chains, and mappings with `ref` or other undeclared
window fields fail ordinary schema validation.

<a id="window-2"></a>

### WINDOW-2. A named reference expands to an independent copy of its definition before analysis

After layer composition, each named reference is replaced by an independent
copy of its definition before semantic dependency analysis and execution. The
expansion keeps the reference's caller scope, including a row template's own
constructed rows, and creates no shared row set. Every operation-specific
restriction applies to the expanded window. The resolved specification
contains only inline windows and omits root `windows`.

Evidence: `rust/crates/yamaa-core/tests/schema_windows.rs::expands_independent_copies_keeps_order_scope_and_definition_origins`, `adam-adlbc-window-chain`, `schema-window-functions`.
Replaces: REQ-1252.

Definition shapes are checked in every written layer, including unused
definitions. Expanded dependencies take part in reachability and column
ordering, and unreachable inherited declarations keep the pruning behavior of
[LAYER-12](inheritance.md#layer-12). Diagnostic provenance keeps the reference
location and attributes expanded fields to their named definition.

## Partitions

<a id="window-3"></a>

### WINDOW-3. A window partitions the rows of its evaluation context by group_by and preserves row count

A window expression partitions the rows of its evaluation context by its
`window.group_by` and preserves row count. In row construction those rows are
the enclosing row template's constructed rows; in an intermediate derivation
they are the donor records as augmented by every derivation declared before
it. Omitting `group_by` creates one partition. Within a declared group missing
values equal other missing values, and rows with equal present values and equal
missing group positions share one partition.

Evidence: `schema-window-functions`, `schema-window-intermediate-rank`, `rust/crates/yamaa-adapters/tests/fixtures/specs/window-failures.json`.
Replaces: REQ-0293, REQ-0326, REQ-1122, REQ-1129.

A window partition is a group of rows, and `window.order_by` orders the rows
within each partition under [Values](values.md). The
intermediate-derivation context is the one of
[LOOKUP-15](lookup.md#lookup-15).

A window expression used during row construction partitions the rows its
enclosing row template constructs; a window that reads rows from another
template or from outside row construction fails. Row derivations follow their
dependency graph. Each derivation completes for all rows of the template
before a dependent derivation starts, so a window may read a completed column
derived from another window, directly or through scalar derivations. A
dependency cycle still fails under [COLUMN-4](columns.md#column-4)
and [COLUMN-6](columns.md#column-6).

<a id="window-4"></a>

### WINDOW-4. A window filter excludes rows before partitioning, and an excluded row receives missing

A window's `filter` is a predicate selecting rows before partitioning, and it
preserves row count: an excluded row receives missing rather than being
dropped. A window that reads a nonexistent partition row returns missing, as
does a window that reads a neighboring row with a missing value. In row
construction, a `window.filter` that is not a Boolean predicate over
constructed output columns fails.

Evidence: `rust/crates/yamaa-core/tests/dataset_windows.rs::window_filters_preserve_rows_and_number_only_true_in_sorted_partitions`, `schema-window-functions`.
Replaces: REQ-0294, REQ-0327.

<a id="window-5"></a>

### WINDOW-5. Positional window operations require window.order_by, and baseline_flag forbids it

`row_number`, `rank`, `row_value` and `locf` require `window.order_by`:
without a declared order the window has no positions to number or to move
along, and omitting it is a validation error.
`baseline_flag` orders its eligible rows by `date` and then by its own
`tie_break` terms, and does not take `window.order_by`; declaring one is a
validation error rather than silently ignored.

Fails: `window_order_by_required`, `window_order_by_forbidden`.
Evidence: `negative-locf-no-order`, `rust/crates/yamaa-adapters/tests/fixtures/window_diagnostics.tsv`, `rust/crates/yamaa-adapters/tests/fixtures/specs/window-failures.json`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs::value_and_tie_break_bind_at_their_paths_and_window_order_stays_forbidden`.
Replaces: REQ-0340, REQ-0341.

## Window operations

<a id="window-6"></a>

### WINDOW-6. row_number numbers positions from 1, and rank gives rows tied on every term one number

`row_number` numbers the eligible rows of each partition from 1 in the
declared order; ordering ties keep row-template order, then base-record order.
`rank` also numbers eligible rows from 1, but compares only the declared
terms: rows equal on every term share one number, two missing values counting
as equal. With `method: competition`, the default, tied rows take the lowest
position they occupy and the next distinct value takes its own position; with
`dense`, the next distinct value takes the following number.

Evidence: `schema-window-functions`, `adam-adae-severity-rank`, `rust/crates/yamaa-core/tests/dataset_windows.rs::numbering_preserves_output_order_exact_integers_nulls_and_partition_ties`.
Replaces: REQ-0303, REQ-1123, REQ-1124.

The tie-break settles positions, not equality. `row_number`, `row_value`,
`locf` and right-side selection read the positions themselves, so a tie
changes which row they reach. `rank` instead gives the records equal on all
declared terms a single number rather than the distinct numbers their
positions would give. The competition method leaves the positions occupied by
a tie out of the subsequent numbers, so intervening numbers are skipped; the
dense method numbers distinct values consecutively, and no number is skipped.
Two missing values are equal for `rank` whatever `nulls` places them among. A
specification that wants a tie broken declares the term that breaks it,
whichever method it uses.

<a id="window-7"></a>

### WINDOW-7. row_value reads the source from the row a nonzero offset away along the declared order

`row_value` returns `source` from another row of the ordered partition.
`offset` counts along the declared order: positive moves toward later rows and
negative toward earlier ones, so reversing every `order_by` direction and
negating `offset` gives the same result. A row with fewer than `|offset|` rows
on that side of it within its partition yields missing, which does not
distinguish an absent row from a present row whose value is missing. An
`offset` of zero fails.

Evidence: `schema-window-functions`, `negative-row-no-prior`, `rust/crates/yamaa-core/tests/dataset_windows.rs::value_window_offsets_use_only_eligible_positions_and_keep_excluded_results_missing`.
Replaces: REQ-0328, REQ-1125.

The current row's own value is `source`, and a window must not be a second
spelling of it.

<a id="window-8"></a>

### WINDOW-8. locf carries the closest strictly earlier non-missing source forward unless include_current lets a present current source answer

`locf` returns `source` from the closest strictly earlier row of the ordered
partition whose source is not missing; missing source values are skipped, so
one result can cross any number of consecutive gaps. With
`include_current: true`, the default, a present current source is returned
instead. With `include_current: false` the current row is never a candidate.
A row with no qualifying source yields missing.

Evidence: `adam-advs-locf`, `adam-adqs-missed-visit-locf`, `adam-advs-carryforward`, `schema-window-functions`, `rust/crates/yamaa-core/tests/dataset_windows.rs::locf_excluding_current_crosses_long_gaps`.
Replaces: REQ-1126, REQ-1239.

`locf` preserves row count and the selected value's type and temporal
precision. Zero and a present empty string are values, not gaps. In either
mode a `window.filter` excludes both donor and recipient rows, and excluded
recipients receive missing. The source is a separate completed column; reading
the column being derived remains a dependency cycle. Consecutive gaps each
select an original observed value, never an already-filled output value.

```yaml
derivation:
  locf:
    source: AVALCOL
    window:
      group_by: [STUDYID, USUBJID, PARAMCD]
      order_by: [AVISITN, ASEQ]
```

With `include_current: false` every row reads the closest earlier observed
value even when its own value is present, as a prior-value column does:

```yaml
derivation:
  locf:
    source: AVALCOL
    include_current: false
    window:
      group_by: [STUDYID, USUBJID, PARAMCD]
      order_by: [ADT, ASEQ]
```

<a id="window-9"></a>

### WINDOW-9. baseline_flag marks the last eligible row ordered by date and then by its tie_break terms

`baseline_flag` returns `Y` for at most one row of each partition and missing
elsewhere. A row is eligible when its `value` is not missing and its `date` is
not missing and falls at or before its `reference_date`, or strictly before it
with `inclusive: false`. The baseline is the last eligible row when eligible
rows are ordered by `date` and then by the `tie_break` terms, each term with
its own direction and missing placement. Eligible rows tied for last on `date`
and every term fail.

Fails: `ambiguous_baseline`, `incompatible_input_type`.
Evidence: `negative-tied-baseline`, `negative-baseline-unresolved-tie`, `schema-baseline-tie-break`, `schema-baseline-exclusive`, `schema-baseline-missing-value`, `schema-window-functions`, `adam-adae-worsening-emergence`, `rust/crates/yamaa-core/tests/dataset_windows.rs::baseline_tie_break_terms_pick_the_last_same_date_row_or_fail_when_still_tied`, `rust/crates/yamaa-core/tests/dataset_windows.rs::baseline_requires_a_present_value_and_honors_an_exclusive_reference`, `rust/crates/yamaa-core/tests/dataset_windows.rs::baseline_filters_exclude_later_rows_and_ties_without_dropping_output`.
Replaces: REQ-1127.

`date` and `reference_date` are both `date` columns or both `datetime`
columns; any other pairing fails validation as `incompatible_input_type` at
the operand. With `datetime` columns the bound and the order use the time of
day as well. `value` may hold any type and only its being missing matters: it
names the analysis value the baseline is for, so the baseline is the last
non-missing value. `inclusive` defaults to `true`. Each row is compared with
its own `reference_date`, and a row whose reference is missing is not
eligible. A partition with no eligible row has no baseline.

`tie_break` is an optional list of order terms under
[VALUE-22](values.md#value-22), compared only between eligible rows that share
the latest date. The last row in that order is chosen, so `direction: desc` on
a term prefers the term's smallest value. A missing tie-break value takes the
term's placement: with `nulls` omitted under `asc` it sorts first, so a row
with a collection time is chosen over a same-day row without one.

`window.group_by` partitions the rows and `window.filter` removes rows before
eligibility is judged ([WINDOW-4](#window-4)); it remains the place for a
compound condition. When two or more eligible rows tie for last on `date` and
every `tie_break` term, the run fails with `ambiguous_baseline`, reporting the
shared date, the number of tied rows and the partition; ties among earlier
rows do not matter.

```yaml
- name: ABLFL
  type: str
  derivation:
    baseline_flag:
      date: ADT
      reference_date: TRTSDT
      value: AVAL
      tie_break: [ATM, LBSEQ]
      window:
        group_by: [STUDYID, USUBJID, PARAMCD]
```

<a id="window-10"></a>

### WINDOW-10. Window operations convert nothing: numbering returns int and selections keep their type

`row_number` and `rank` return integers, and `baseline_flag` returns a string.
`row_value` and `locf` accept any `source` type, perform no coercion, and
return the selected value with its type and, for a temporal value, its
collected precision. `row_value` requires an integer `offset`.

Evidence: `schema-window-functions`, `rust/crates/yamaa-adapters/tests/fixtures/source_selection.json`.
Replaces: REQ-0311, REQ-0316.

Scalar selection follows [EXPR-6](expressions.md#expr-6), and temporal
operation results follow [TIME-1](temporal.md#time-1).
