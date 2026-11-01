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

Fails: `invalid_field_type`.
Evidence: `schema-window-functions`, `schema-window-intermediate-rank`, `negative-window-repeated-group`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::a_window_order_over_an_int_and_a_float_numbers_the_records_by_exact_value`.

A `window.group_by` names each variable once; a variable listed again fails as
`invalid_field_type` at that entry, reporting the variable. A window partition
is a group of rows, and `window.order_by` orders the rows within each partition
under [VALUE-22](values.md#value-22). The intermediate-derivation context is
the one of [LOOKUP-15](lookup.md#lookup-15), where an earlier untyped
derivation may hold an `int` or a `float`, which order by exact value.

A window expression used during row construction partitions the rows its
enclosing row template constructs; a window that reads rows from another
template or from outside row construction fails. Row derivations follow their
dependency graph. Each derivation completes for all rows of the template
before a dependent derivation starts, so a window may read a completed column
derived from another window, directly or through scalar derivations. A
dependency cycle still fails under [COLUMN-4](columns.md#column-4)
and [COLUMN-6](columns.md#column-6). A window in a column-level derivation
partitions the rows of every row template, and
[WINDOW-11](#window-11) limits the driver fields it reads.

A column-level backward `row_value` may instead read calculated history under
[WINDOW-12](#window-12), after its partition and order are independently
complete.

<a id="window-11"></a>

### WINDOW-11. A column-level window reads a driver field only when every row template has that driver

A window in a column-level derivation partitions and orders the rows of every
row template, and reads a qualified field at the record that built each row.
It may name a field of a driver only when every row template, grouped or not,
has that driver, and [NAME-4](names.md#name-4) still decides what a grouped
row holds. When the row templates do not all share one driver, a field of one
of their drivers, or of the input a driver intermediate reads, fails.

Fails: `unknown_field`.
Evidence: `negative-window-other-driver`, `sdtm-fa-reactogenicity-diary`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/window_drivers.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_scopes.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_column_window_reads_no_field_of_a_driver_one_row_template_lacks`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_column_window_may_read_the_driver_every_row_template_shares`.

A column-level window is evaluated once over every constructed row, whichever
row template built it; [SPEC-6](specification.md#spec-6) lets no row template
derive its column. A qualified field it names is read from the record each row
was built from: the driver record, or for a grouped row template the records of
the row's group. A row built from another driver holds no such record, and a
window reads no implicit join ([LOOKUP-1](lookup.md#lookup-1)), so for that
row the field names nothing it holds. A driver intermediate's records are
records of the input it reads, but a row it builds holds them as the
intermediate's ([ROW-2](rows.md#row-2)), and a field of that input names
nothing the row holds either.

The read fails validation as `unknown_field` at the operand that names it: the
operation's `source`, `date` or `reference` entry, an entry of
`window.group_by` or `window.order_by`, or `window.filter` for each qualified
name the filter reads. A window in a `case` result
([EXPR-3](expressions.md#expr-3)) and a named window
([WINDOW-2](#window-2)) are checked where they are used. The finding names the
field as `identifier`, the distinct drivers of the row templates in
construction order as `drivers`, and the window operation as `operation`.

Bring such a value onto every row as its own column, which each row completes
in its own scope, and name the column. A column-level `source` reads the
record of the row's own driver and joins another input on that input's
declared `key` ([LOOKUP-1](lookup.md#lookup-1)), and a row template may derive
the column itself:

```yaml
columns:
  - name: AGE
    type: int
    derivation: DM.AGE
  - name: ORD
    type: int
    derivation:
      row_number:
        window:
          order_by: [AGE]
```

When every row template has the driver, the window reads the field from the
record that built each row. A grouped row template over that driver then
holds only its group keys, and a read of another field of the driver fails as
`ungrouped_driver_field` under [NAME-4](names.md#name-4).

<a id="window-4"></a>

### WINDOW-4. A window filter excludes rows before partitioning, and an excluded row receives missing

A window's `filter` is a predicate selecting rows before partitioning, and it
preserves row count: an excluded row receives missing rather than being
dropped. A window that reads a nonexistent partition row returns missing, as
does a window that reads a neighboring row with a missing value. In row
construction, a `window.filter` that is not a Boolean predicate over
constructed output columns fails.

Evidence: `schema-window-functions`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_baseline_flag_s_filtered_later_row_takes_no_baseline_from_an_earlier_date`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_baseline_flag_s_filtered_same_date_twin_does_not_tie`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::filtered_and_ineligible_rows_do_not_end_the_baseline_search`.

<a id="window-5"></a>

### WINDOW-5. Positional window operations require window.order_by, and baseline_flag defaults it to its date

`row_number`, `rank`, `row_value` and `locf` require `window.order_by`:
without a declared order the window has no positions to number or to move
along, and omitting it is a validation error. `baseline_flag` also takes
`window.order_by`, which defaults to its `date` alone; a declared order starts
with that `date` ascending, and its later terms break ties
([WINDOW-9](#window-9)).

Fails: `window_order_by_required`, `value_not_permitted`.
Evidence: `negative-locf-no-order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::a_window_order_over_text_and_an_int_is_refused_before_any_record_is_read`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_window_order_term_whose_derived_type_holds_a_date_and_a_datetime_is_refused`, `rust/crates/yamaa-adapters/tests/declaration_v1_lowering.rs::a_declared_baseline_order_starts_with_the_date_ascending`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/one_spelling.json::a_baseline_order_starting_with_another_variable_is_refused_at_that_term`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/one_spelling.json::a_baseline_order_starting_with_the_date_descending_is_refused_at_that_term`, `rust/crates/yamaa-application/tests/fixtures/window_diagnostics.tsv`.

A `baseline_flag` order whose first term is not its `date` ascending fails
validation as `value_not_permitted` at that term, its context naming the
term's variable, its direction, the `date` and the refused `value`: the
variable, or `desc` when the term orders the `date` descending.

Each term of the order, a baseline's `date` and tie-break terms included,
sorts under [VALUE-22](values.md#value-22): a term whose field's types do not
all compare, which only an untyped intermediate derivation can give it
([LOOKUP-15](lookup.md#lookup-15)), fails validation as
`incompatible_input_type` at the term, naming the window operation as
`operation`.

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

Evidence: `schema-window-functions`, `adam-adae-severity-rank`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::numbering_and_rank_order_integers_above_2_53_exactly_with_missing_values_last_in_one_missing_partition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::numbering_and_rank_place_missing_values_first_under_a_descending_order_that_declares_it`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::rank_orders_floats_numerically_so_negative_and_positive_zero_tie`.

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

Fails: `zero_offset`.
Evidence: `schema-window-functions`, `negative-row-no-prior`.

The current row's own value is `source`, and a window must not be a second
spelling of it.

<a id="window-12"></a>

### WINDOW-12. Strictly backward row_value reads can carry calculated history in one independently ordered column group

A column-level `row_value` with a negative `offset` may name a calculated
column, including itself or one declared later. A feedback group is admitted
when removing its backward source dependencies leaves an acyclic current-row
graph, every member has one typed column binding for all output rows, and all
its window reads are backward `row_value` operations over the same partition
keys and order terms, without a window filter. Its partition and order columns
are completed outside the group. An unsupported cycle still fails.

Fails: `dependency_cycle`, `forward_reference`.
Evidence: `adam-advs-dosing-weight`, `negative-row-value-self`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/calculated_history.json`.

Keys, row derivations, intermediate donor derivations, context windows,
lookups, aggregates and project functions cannot participate in a feedback
group.

The group completes after all its external dependencies. Partitions run in
first-occurrence order and their rows in the declared stable window order;
within each row, current-row dependencies determine the column order. Each
value is evaluated, converted and handled before the next value reads it
([COLUMN-8](columns.md#column-8)). A backward read selects that completed
value, including its being missing. A position before the partition starts
returns missing; it does not initialize state, skip a missing result, or
search a preceding partition. Initialization and missing-observation policies
belong to the ordinary expressions that use the read.

The first failure in that causal order stops the group, with only the handler
firings preceding it counted. Whole-column verifications run after the group
completes, before an outside consumer proceeds, in the ordinary declared check
order. Unrelated columns keep their existing evaluation and failure order.

Every attempt owns its calculated values and window positions. No state is
retained by a prepared plan or a previous run. Calculated-history groups run
whole-table; they are not admitted to partitioned input execution. Ordinary
windows over completed columns retain their existing partition support.

<a id="window-8"></a>

### WINDOW-8. locf carries the closest strictly earlier non-missing source forward unless include_current lets a present current source answer

`locf` returns `source` from the closest strictly earlier row of the ordered
partition whose source is not missing; missing source values are skipped, so
one result can cross any number of consecutive gaps. With
`include_current: true`, the default, a present current source is returned
instead. With `include_current: false` the current row is never a candidate.
A row with no qualifying source yields missing.

Evidence: `adam-advs-locf`, `adam-adqs-missed-visit-locf`, `adam-advs-carryforward`, `schema-window-functions`.

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

### WINDOW-9. baseline_flag flags the last eligible row of each partition in date order, and a tie for last fails

`baseline_flag` returns `Y` for at most one row of each partition and missing
elsewhere. A row is eligible when its `source` is not missing and its `date` is
not missing and falls at or before its `reference`, or strictly before it with
`inclusive: false`. The baseline is the last eligible row when eligible rows
are ordered by `window.order_by`, each term with its own direction and missing
placement. Eligible rows tied for last on every term fail.

Fails: `ambiguous_baseline`, `incompatible_input_type`.
Evidence: `negative-tied-baseline`, `negative-baseline-unresolved-tie`, `schema-baseline-tie-break`, `schema-baseline-exclusive`, `schema-baseline-missing-value`, `schema-window-functions`, `adam-adae-worsening-emergence`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_descending_time_tie_break_prefers_the_smallest_time_and_keeps_a_missing_time_lowest`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_tie_break_declaring_nulls_last_places_a_missing_time_last`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_descending_sequence_tie_break_picks_the_lowest_sequence_number`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::same_date_rows_still_tied_on_every_tie_break_term_fail_naming_the_date_the_count_and_the_partition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::baseline_flag_judges_each_row_by_its_own_reference_within_its_partition`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::the_latest_eligible_date_is_the_baseline`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_date_after_its_reference_and_a_missing_date_are_never_eligible_for_baseline_flag`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_same_date_row_after_its_own_reference_is_not_eligible_for_baseline_flag`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::a_date_equal_to_its_own_reference_is_eligible_for_baseline_flag`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/windows.json::ineligible_rows_do_not_end_the_baseline_search`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::a_baseline_date_over_a_date_and_a_datetime_is_refused_before_any_record_is_read`.

`date` and `reference` are both `date` columns or both `datetime` columns;
any other pairing fails validation as `incompatible_input_type` at the
operand. A `date` whose type an untyped intermediate derivation decides per
value between `date` and `datetime` is an order term whose types do not
compare, and fails under [VALUE-22](values.md#value-22) at the `date`. With `datetime` columns the bound and the order use the time of day
as well. `source` may hold any type and only its being missing matters: it
names the analysis value the baseline is for, so the baseline is the last
non-missing value. `inclusive` defaults to `true`. Each row is compared with
its own `reference`, and a row whose reference is missing is not eligible. A
partition with no eligible row has no baseline.

`window.order_by` holds order terms under [VALUE-22](values.md#value-22) and
defaults to the `date` alone ([WINDOW-5](#window-5)); the terms after the
`date` are compared only between eligible rows that share the latest date.
The last row in that order is chosen, so `direction: desc` on a term prefers
the term's smallest value. A missing tie-break value takes the
term's placement: with `nulls` omitted under `asc` it sorts first, so a row
with a collection time is chosen over a same-day row without one.

`window.group_by` partitions the rows and `window.filter` removes rows before
eligibility is judged ([WINDOW-4](#window-4)); it remains the place for a
compound condition. When two or more eligible rows tie for last on every
term of `window.order_by`, the run fails with `ambiguous_baseline`, reporting
the shared date, the number of tied rows and the partition; ties among
earlier rows do not matter.

```yaml
- name: ABLFL
  type: str
  derivation:
    baseline_flag:
      date: ADT
      reference: TRTSDT
      source: AVAL
      window:
        group_by: [STUDYID, USUBJID, PARAMCD]
        order_by: [ADT, ATM, LBSEQ]
```

<a id="window-10"></a>

### WINDOW-10. Window operations convert nothing: numbering returns int and selections keep their type

`row_number` and `rank` return integers, and `baseline_flag` returns a string.
`row_value` and `locf` accept any `source` type, perform no coercion, and
return the selected value with its type and, for a temporal value, its
collected precision. `row_value` requires an integer `offset`.

Evidence: `schema-window-functions`, `rust/crates/yamaa-application/tests/fixtures/source_selection.json`.

Scalar selection follows [EXPR-6](expressions.md#expr-6), and temporal
operation results follow [TIME-1](temporal.md#time-1).
