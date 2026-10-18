# Lookup and joins

A derivation reads another relation through the implicit join of a
dataset-qualified source, through a named intermediate declared in
`intermediates`, or through a qualified aggregate. Each is one lookup
([LOOKUP-25](#lookup-25)): it matches records on keys and an optional range,
narrows them by a filter, selects one record or reduces them, and answers
absence. The kinds of lookup differ only in the steps they declare.

## The lookup model

<a id="lookup-25"></a>

### LOOKUP-25. Every lookup runs the same four steps, match, narrow, select and answer absence, whatever its kind

An implicit join, a named intermediate and a qualified aggregate are one
lookup in four steps, run in order for each current row. Match keeps the
records whose keys equal the row's match values and whose bounds, when
declared, hold its range value ([LOOKUP-9](#lookup-9)). Narrow keeps the
matched records its `filter` holds for ([LOOKUP-8](#lookup-8)). Select chooses
one candidate, or an aggregate reduces them ([LOOKUP-10](#lookup-10)). With no
candidate, absence is answered ([LOOKUP-24](#lookup-24)). Candidates keep
stored record order. The kinds differ only in the steps they enable.

Evidence: `schema-lookup-source-range`, `schema-lookup-filters`, `schema-aggregation`, `rust/crates/yamaa-adapters/tests/execution_lookup_model.rs`.

| Step | Implicit join | Named intermediate | Qualified aggregate |
| --- | --- | --- | --- |
| Match keys | The applicable output keys ([LOOKUP-1](#lookup-1)) | `key`, or the applicable keys ([LOOKUP-5](#lookup-5)) | `key`, or the applicable keys |
| Match range | `between` on a structured `source:` | `between` | `between` |
| Narrow | `filter` on a structured `source:`, over the donor record | `filter`, which may also read the current row's driver | `filter`, over the donor record |
| Select | `order_by` and `keep` | `order_by` and `keep` | The reduction of `expr` over every candidate ([LOOKUP-18](#lookup-18)) |
| Answer absence | `absent`, missing by default, or `require_match` | `no_match`, missing by default, or `require_match` | Missing, uncounted ([FORMULA-24](formulas.md#formula-24)) |

A step a lookup does not declare keeps every record: without `between` every
record whose keys match is in range, and without `filter` every matched record
is a candidate. Each step reads only what the step before kept, so a record is
narrowed only after it matches, and selection or reduction sees only the
narrowed records. A key value whose type cannot compare therefore fails even
on a record the filter would remove ([LOOKUP-9](#lookup-9)), and a filter that
would fail on a record no row matches is never asked about it
([LOOKUP-8](#lookup-8)). A named intermediate that verifies a unique
combination ([LOOKUP-17](#lookup-17)) or drives a row template
([ROW-2](rows.md#row-2)) reads every record its filter keeps, so there its
filter is asked about every record.

The current row's match values and range value are evaluated before any
record is read, so a failure there is reported before any record is matched
or narrowed. A missing one seeks no record, and absence is answered.

Each kind adds only what it needs. An implicit join infers its keys and
declares no `key` ([LOOKUP-1](#lookup-1)). A named intermediate is read
through its id, and every column reading it for a row shares one selection
([LOOKUP-12](#lookup-12)). An aggregate reduces every candidate instead of
selecting one, so it declares no `order_by` or `keep`. An unqualified
aggregate and a grouped-row aggregate are not lookups: they reduce the
constructed output rows of a partition or the current input group, which
their `filter` narrows ([LOOKUP-8](#lookup-8)), and declare no `between`
([LOOKUP-19](#lookup-19)).

<a id="lookup-9"></a>

### LOOKUP-9. Match keeps the records whose keys equal the current row's match values and whose bounds hold its range value

Step one keeps each record whose every key column equals its paired match
value and, with `between`, whose stated bounds hold the range value:
`lower <= value` and `value <= upper`, both inclusive. Values compare exactly
under [VALUE-19](values.md#value-19), with no operand converted, and a pair or
bound whose types cannot compare fails. A missing match or range value seeks
no record, and a record missing a key or a stated bound does not match.
`between` names a current-row value and at least one bound field.

Fails: `incompatible_input_type`, `incomparable_range_types`, `invalid_field_type`, `prohibited_construct`, `unknown_field`.
Evidence: `negative-lookup-bad-range`, `negative-lookup-range-other-input`, `negative-aggregate-range-other-input`, `schema-lookup-source-range`, `schema-lookup-source-range-driver`, `schema-lookup-intermediate-range-driver`, `schema-aggregate-range-driver`, `sdtm-lb-lab-specific-ranges`, `adam-advs-window-table`, `adam-adtr-nadir`, `adam-adsl-new-therapy`, `rust/crates/yamaa-adapters/tests/execution_lookup_model.rs`.
Replaces: REQ-0121, REQ-0128, REQ-0134, REQ-0330, REQ-0472, REQ-0473, REQ-0474,
REQ-0509, REQ-1049.

The key pairs are those of [LOOKUP-5](#lookup-5). Every pair is compared on
every record offered, so a record value whose type cannot compare with its
match value fails whatever the other pairs hold, even on a record the filter
would remove. A pair whose declared types, or the result type of its match
expression ([LOOKUP-6](#lookup-6)), cannot compare fails as
`incompatible_input_type` before any record is read, reporting both types, for
every kind of lookup; a type decided per value fails the same way when that
value is compared. `int` and `float` compare exactly, and every other type
must match exactly.

`between` declares `value` and `lower`, `upper` or both, each bound a field of
the lookup's relation: a stored field, or a name an intermediate derives
([LOOKUP-15](#lookup-15)). An omitted bound leaves that side unbounded without
excluding the stated endpoint. Every kind of lookup may declare it: a named
intermediate, a qualified aggregate, and an implicit join through a structured
`source:` binding or a filtered source ([LOOKUP-1](#lookup-1)). The range is
compared only on the records whose keys match, and a record missing a stated
bound is ineligible rather than open-ended. A missing range value seeks no
record; it never widens the match to every record whose keys match.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
    between: {value: AGE, lower: AGELO, upper: AGEHI}
```

The value names what the current row holds: an output column, written bare; a
field of the record of the row's driver, qualified by the driver; or a column
a named intermediate selects for the row, qualified by its id. A value read
from a driver's record must name the driver of every row that reads the
lookup, wherever the lookup is read: in a row template, that template's driver
([LOOKUP-2](#lookup-2)), and during column derivation, the driver of every row
template the read can evaluate on: the templates that leave the column to the
column phase. A row template's `filter` reads its lookups for that template's
rows, and a dataset verification for the rows of every template. A named
intermediate is also read through an aggregate's `derive` binding and through
the range value of another lookup. A value naming nothing the current row
holds, such as a field of another input or of the lookup's own relation, fails
as `unknown_field` at `between.value`, once per range, whichever kind of
lookup declares it. Which input drives each row is declared, so `check`
reports a value qualified by an input that drives none of the rows reading the
lookup from the declarations alone, whether or not that input stores the
field. A field of another input is never read through the implicit join
([LOOKUP-1](#lookup-1)); bring it onto the row as a column first, and name the
column:

```yaml
columns:
  - name: AGE
    type: int
    derivation: DM.AGE
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD]
    between: {value: AGE, lower: AGELO, upper: AGEHI}
```

The value and every stated bound must be mutually comparable, with no operand
converted. A value and a bound whose types cannot compare fail validation as
`incomparable_range_types`, reporting the types of the value and the stated
bounds; a type decided per value fails the same way when the record is
compared. A derived bound has its derivation's declared or result type
([LOOKUP-15](#lookup-15)).

A `between` with neither bound fails as `invalid_field_type`, and a bound
naming no field of the relation as `unknown_field`. A source that reads the
driver's own record, an output column or a named intermediate has no implicit
join to narrow, and `between` there fails as `prohibited_construct` at the
field. A `between` on an unqualified or grouped-row aggregate, which matches
no relation, fails as `invalid_aggregate_context` ([LOOKUP-18](#lookup-18)).

<a id="lookup-8"></a>

### LOOKUP-8. Narrow keeps the matched records for which the lookup's filter is TRUE

Step two asks a lookup's `filter` about each matched record, and only about
matched records, keeping those for which it is `TRUE` in stored order. Its
identifiers must name available fields in an admitted scope, or fail: the
lookup's qualifier reads the donor record, and a named intermediate's filter
may also read the current row's driver. An aggregate's filter narrows its own
relation, whichever it reduces. A filter on a construct with no records to
select among fails.

Fails: `unknown_field`, `prohibited_construct`.
Evidence: `schema-lookup-filters`, `negative-source-missing-field`, `negative-source-trivial-filter`, `negative-row-filter-lookup-scope`, `schema-aggregation`, `rust/crates/yamaa-adapters/tests/execution_lookup_model.rs`, `rust/crates/yamaa-adapters/tests/execution_lookup_read_contexts.rs`, `rust/crates/yamaa-adapters/tests/fixtures/row_reductions.json`.
Replaces: REQ-0132, REQ-0133, REQ-0148, REQ-0471.

For an input dataset, `order_by` names qualified fields of that dataset only,
`columns` lists bare field names, and every `filter` field is qualified. The
current row's driver is root `base` (or the sole input) without explicit rows,
or the enclosing `rows[].dataset` during row construction. During column
derivation the qualifier must be the driver of every row template that the
expression can evaluate on. A row template's `filter` reads its lookups for
that template's rows, and a dataset verification for the rows of every
template, as a column derivation does. The row driver may be an eligible named
intermediate under [ROW-2](rows.md#row-2). Other datasets,
intermediates that are not the current driver, and unqualified output names
are not filter scopes. The filter of a structured source or a qualified
aggregate reads the donor record only; correlated predicates are supported by
named intermediates.

The current-driver references are dependencies of each lookup read and obey
the existing phase and group-key rules: in a grouped template, the qualified
field must be a group key. An unknown field fails as `unknown_field`, and when
the bare field exists in the donor dataset the diagnostic suggests its
qualified spelling. `order_by` remains donor-only even when `filter` is
correlated. When driver and donor use the same dataset identifier, that
qualifier always names the donor; declare a separate input identifier for the
same resource when both sides need to be referenced explicitly.

```yaml
base: PLAN
intermediates:
  - id: PRIOR
    dataset: OBS
    key: [STUDYID, USUBJID, PARAMCD]
    filter: >-
      OBS.ANL01FL = 'Y' AND OBS.AVAL IS NOT NULL
      AND OBS.AVISITN < PLAN.AVISITN
    order_by: [OBS.AVISITN, OBS.QSSEQ]
    keep: last
```

The key match finds the subject's records of the parameter, the comparison
keeps those before the current visit, and ordered selection chooses one.
`PRIOR.AVAL`, `PRIOR.ADT` and other reads share the chosen record, including
its missing fields. The lookup generates no rows.

`FALSE` and `UNKNOWN` exclude a candidate. A filter that reads only the donor
record gives one answer for a record whichever row matched it, and that answer
may be reused; a correlated result is never reused for another row. Normal
predicate typing and missing-value rules apply without coercion. The filter is
asked only about matched records, so a filter that would fail on a record no
row matches never runs as a lookup. A named intermediate's unique verification
([LOOKUP-17](#lookup-17)) and a row template it drives
([ROW-2](rows.md#row-2)) read every record the filter keeps, so they ask it
about every record. A `filter` on a construct with no records to select among
(an output column, a chosen lookup record, a group key) fails as
`prohibited_construct`.

An aggregate's `filter` narrows the relation it reduces: for a qualified
aggregate, the right-side records its lookup matches; for an unqualified one,
the constructed output rows of each `group_by` partition; and for a
grouped-row aggregate, the records of the current input group.
[FORMULA-4](formulas.md#formula-4) defines predicate evaluation, and filtering
preserves the order of the retained records
([FORMULA-19](formulas.md#formula-19)).

<a id="lookup-10"></a>

### LOOKUP-10. Select chooses the first or last candidate by order_by, and an aggregate reduces every candidate

Step three chooses one of several candidates. `order_by` and `keep` are
declared together or not at all; with them, `keep: first` chooses the
candidate that sorts first and `keep: last` the one that sorts last, a tie
going to the earlier stored record for `first` and the later for `last`. More
than one candidate with nothing to choose by is an unhandled multiple match
and fails. An aggregate declares no selection and reduces every candidate, in
stored order.

Fails: `unpaired_fields`, `multiple_matches`.
Evidence: `negative-lookup-unordered`, `negative-lookup-empty-order`, `negative-lookup-unchosen`, `negative-source-duplicate-key`, `rust/crates/yamaa-adapters/tests/execution_lookup_model.rs`.
Replaces: REQ-0119, REQ-0127, REQ-0135, REQ-0145.

Declaring one of `order_by` and `keep` without the other fails as
`unpaired_fields`, on a named intermediate and a source binding alike, and an
unhandled multiple match fails as `multiple_matches`. An `order_by` that lists
no term orders nothing: `keep` beside it fails as `unpaired_fields` the same
way, reporting `order_by` missing. During grouped row
construction it fails at the join phase, before any column is derived. A
`keep` that chose among several candidates counts one `multiple_matches`
handling ([COLUMN-14](columns.md#column-14)).

The `order_by` terms sort under [VALUE-22](values.md#value-22) and name fields
of the lookup's relation, in the scopes of [LOOKUP-8](#lookup-8). A lookup
whose key is verified unique needs neither `order_by` nor `keep`
([LOOKUP-17](#lookup-17)). An aggregate reduces its candidates in stored order
([FORMULA-19](formulas.md#formula-19)).

<a id="lookup-24"></a>

### LOOKUP-24. A lookup with no candidate answers absence, missing by default, unless require_match fails it

A lookup with no candidate for a row, or with a missing match or range value,
answers absence: an implicit join gives its `source` binding's `absent` value
and a named intermediate its `no_match` value, either missing when undeclared
and counted under that handler unless a predicate made the read.
`require_match: true` on the binding or intermediate, or on an input for joins
that declare no `absent`, instead fails the read as `unmatched_key`; one
lookup cannot declare both. An aggregate's empty match yields missing
([FORMULA-24](formulas.md#formula-24)).

Fails: `unmatched_key`, `conflicting_absence_policy`, `prohibited_construct`.
Evidence: `schema-lookup-absent-default`, `negative-source-require-match`, `negative-input-require-match`, `negative-lookup-unmatched-key`, `negative-lookup-missing-sex`, `negative-lookup-require-match-no-match`, `schema-handlers`, `adam-adae-death`, `rust/crates/yamaa-adapters/tests/execution_implicit_join_absence.rs`, `rust/crates/yamaa-adapters/tests/fixtures/original_source_bindings.json`.
Replaces: REQ-0124, REQ-0129, REQ-0130, REQ-0131, REQ-0147.

```yaml
input:
  ADSL: {path: adsl.csv, require_match: true}
  EX: ex.csv
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
    require_match: true
columns:
  - name: TRTSDT
    derivation:
      source:
        variable: EX.EXSTDTC
        filter: "EX.EXDOSE > 0"
        order_by: [EX.EXSTDTC]
        keep: first
        require_match: true
```

The concise `DATASET.VARIABLE` is a source binding's shorthand and answers by
default, counted at the binding's path. Any other read through an implicit
join, such as an operation's variable operand, declares no `absent` and gives
missing, counted under `absent` at its own path. A read made while a `flag`
condition or a `case` predicate is evaluated is not a handler site: it gives
missing uncounted, and the predicate answers the unknown result by its own
rule ([EXPR-8](expressions.md#expr-8)). Counts follow
[COLUMN-14](columns.md#column-14), and a declared `absent` is counted the same
way as the default.

A named intermediate with no candidate gives every column reading it its
`no_match` literal, or missing when `no_match` is omitted, independently for
each column read and counted under that column's `no_match` handler, declared
or by default. One column's handling never answers for another.
`no_match: {literal: null}` answers with missing, the same answer as an
omitted `no_match`, and it keeps that meaning when an empty child inherits the
specification. A YAML null declares no handler: written over an inherited
`no_match` it clears the handler, and the read answers with missing again,
under [LAYER-9](inheritance.md#layer-9).

An aggregate declares no absence handler and no `require_match`. A row whose
match leaves no candidate, or whose match or range value is missing, reduces
an empty group, which yields missing under the empty-group rule of
[FORMULA-24](formulas.md#formula-24); no handler counts it.

A missing match or range value is incomplete, not unmatched: no record is
sought, and the row is answered, or fails, the same way. A matched record
whose value is missing is a collected blank, not an absent record, and is not
answered. `absent` keeps answering a variable or ODM item that does not exist
([COLUMN-11](columns.md#column-11)), and with several candidates `order_by`
and `keep` choose one first ([LOOKUP-10](#lookup-10)).

`require_match: true` asserts that every row the read is made for finds a
record. On a binding it covers that binding, and on a named intermediate every
read of it. On an input, `input.<NAME>.require_match` covers every read
through an implicit join to that input, the concise form, operands and
predicate reads included, and it may be declared on an input no join reads. A
binding's declared `absent` still answers its own reads, and a named
intermediate over the input keeps its own absence policy: `no_match`,
`require_match` or neither. A row that finds no record fails as
`unmatched_key` at the read's path, reporting the sought key values in the
vocabulary of [LOOKUP-14](#lookup-14), and a read that requires a match counts
nothing.

`require_match: true` beside `absent` on one binding, or beside `no_match` on
one intermediate, declares both that a row without a record is an error and
what it gives, and fails validation as `conflicting_absence_policy` at the two
fields: choose one, or add the condition that keeps the rows that must match.
A source that reads the driver's own record, an output column or a named
intermediate has no implicit join to require a match of, and `require_match`
there fails as `prohibited_construct` at the field; a named intermediate
declares its own.

<a id="lookup-14"></a>

### LOOKUP-14. Lookup handler counts, failures and reviews report the match in one vocabulary

`key` and `intermediate_key` name the fields a lookup matched on and the values
it matched them with, and `keys` names the output row a failure belongs to.
An unmatched key and an unhandled multiple match report the match the same
way, and `unmatched_key` carries the sought source values. Validation reports
the source/key pairs of every lookup and implicit join: the match performed,
the type each side declares, whether the pairs were inferred or declared, and
the absence policy that answers a miss.

Evidence: `negative-lookup-unmatched-key`, `rust/crates/yamaa-adapters/tests/fixtures/specs/lookup-failures.json`.
Replaces: REQ-0139, REQ-0143, REQ-0144, REQ-0149.

Neither failure renames the other's fields. Handler accounting follows
[COLUMN-14](columns.md#column-14), whose counts are `handler` rows of the run
report: a `keep` that chose among surviving
records counts one `multiple_matches` handling, an absence an intermediate
answers counts one `no_match` handling, declared or by default, and a record
an implicit join does not find counts one `absent` handling at the read's path
([LOOKUP-24](#lookup-24)). An `unmatched_key` from an implicit join names
`intermediate(<dataset>)` as its intermediate, as an implicit join's multiple
match does.

## Keys

<a id="lookup-5"></a>

### LOOKUP-5. A key pairs each column of the lookup's relation with a current-row match value, and an omitted key is the applicable output keys

`key` states the match as pairs, each a column of the lookup's relation and
the current-row match value it must equal. A list of column names pairs each
with the same-named current-row value; a mapping pairs each column, written as
its key, with a variable or expression. An implicit join, and a named
intermediate or qualified aggregate without `key`, pair the applicable output
keys ([LOOKUP-1](#lookup-1)). A lookup names at least one pair, every column
must exist, and every match value must read a known current-row value.

Fails: `invalid_field_type`, `no_applicable_keys`, `unknown_field`, `invalid_aggregate_context`.
Evidence: `negative-lookup-mismatched`, `negative-lookup-unknown-key`, `negative-lookup-repeated-key`, `negative-lookup-key-other-template`, `negative-aggregate-key-other-template`, `negative-verification-lookup-key`, `negative-row-aggregate-keys`, `sdtm-lb-lab-specific-ranges`, `rust/crates/yamaa-adapters/tests/fixtures/intermediate_key_reference.json`, `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-adapters/tests/execution_lookup_read_contexts.rs`, `rust/crates/yamaa-adapters/tests/execution_aggregate_key_reads.rs`.
Replaces: REQ-0115, REQ-0116, REQ-0117, REQ-0118, REQ-0140, REQ-0141, REQ-0142,
REQ-0305.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: {TESTCD: LBTESTCD, SEX: SEX}
```

Here the current-row `LBTESTCD` matches the limit table's `TESTCD` column; the
list `[TESTCD, SEX]` would have matched `TESTCD` against a current-row
`TESTCD` that does not exist. A list of one column is still a list. A mapping
may pair a column with the same-named value, and a match value may be an
expression ([LOOKUP-6](#lookup-6)). Pair order does not change the result.

The applicable keys are the output `keys`, in output-key order, that the
lookup's relation also carries, each paired with the current row's same-named
value. An implicit join always matches them. A named intermediate or a
qualified aggregate matches them when it omits `key`, and states `key` only
when the intended match differs. With no applicable key the intended match is
unclear, and the lookup fails as `no_applicable_keys`, reporting the dataset
and the output keys with a hint: declare the `key` explicitly, or, for an
implicit join, read the dataset through a named intermediate that declares
it.

A written `key` names at least one pair. An empty list or mapping names no
match and fails as `invalid_field_type`, on a named intermediate and an
aggregate alike. A named intermediate's list names each column once; a column
listed again fails as `invalid_field_type` at that entry, reporting the
column.

A `key` column the lookup's relation does not have fails as `unknown_field`.
Every match value must read a known current-row value: a variable must name
one, and every identifier an expression reads must be one, or the match fails
as `unknown_field`. `SELF` names donor rows and is never a current-row scope,
so a match value written `SELF.field` names no current-row value and fails the
same way; when the current row carries `field`, the diagnostic suggests that
bare name.

A match value read from an input's record names the driver of every row that
reads the lookup, wherever the lookup is read: in a row template's derivations
or row `filter`, that template's driver ([LOOKUP-2](#lookup-2)), and in a
column derivation or a dataset verification, the driver of every row template
the read can evaluate on. A match value reading the record of another input
names no value of those rows and fails as `unknown_field` once for the lookup,
however many row templates read it: at a named intermediate's `key`, and at
a qualified aggregate's pair, `key.<column>`.

Each pair compares its match value with its column in the match step
([LOOKUP-9](#lookup-9)): the two types must be mutually comparable under
[VALUE-19](values.md#value-19), with no operand converted, or the pair fails
as `incompatible_input_type` under [VALUE-2](values.md#value-2).
[INPUT-5](inputs.md#input-5) gives an undeclared field of a typeless container
the type `str`; a key typed on one side and defaulted on the other needs
repair, not a wider comparison.

A grouped-row aggregate reads its own input group and declares no key pairs:
the group is the match, and a `key` there fails as `invalid_aggregate_context`
([LOOKUP-19](#lookup-19)).

<a id="lookup-6"></a>

### LOOKUP-6. A key match value may be an expression whose static result type is checked before matching

A match value in a `key` mapping may be an expression instead of a variable.
It is evaluated against the current row, its value is the match operand for
that position, and a missing result matches nothing, as a missing variable
does. The pair's comparison ([LOOKUP-9](#lookup-9)) uses the expression's
result type ([EXPR-14](expressions.md#expr-14)) before any record is matched;
a result type decided per value is compared when each value is matched.

Evidence: `adam-adlb-supplb-padded-key`, `adam-adlb-end-of-treatment`, `rust/crates/yamaa-core/tests/relations_aggregate_compiler.rs::key_expressions_compare_by_their_one_result_type_before_matching`.
Replaces: REQ-1259.

The result type is the expression's ordinary non-missing result: a local
`missing`, `invalid` or `no_match` replacement takes no part, and a non-missing
replacement is compared when it is used as a match operand. A pair fails
before matching only when none of the types the result can take compares with
the key column's type under [VALUE-19](values.md#value-19), and the failure
names that result type. `N + 1` over an `int` `N` is an `int`, a `mapping` to
integer values an `int`, and `greatest` over an `int` and a `float` a number of
either type, which compares with a numeric key column and with no text one.
`{literal: null}` is always missing and compares with every column. The same
holds for a named intermediate's and an aggregate's `key` alike.

## The implicit join

<a id="lookup-1"></a>

### LOOKUP-1. A dataset-qualified source reads through the implicit join on the applicable output keys

A scalar source qualified by a dataset reads it through the implicit join, a
lookup whose key pairs are the applicable keys: the output `keys`, in
output-key order, that the dataset also carries, each matched against the
current row's same-named value. A structured `source:` binding adds its
`between`, `filter`, `order_by`, `keep`, `absent` and `require_match` to the
join ([LOOKUP-25](#lookup-25)). The join gives one value per current row and
preserves the row. The qualifier of the row template's driver is not a join.

Evidence: `schema-lookup`, `schema-source-binding`, `schema-lookup-source-range`, `schema-lookup-source-range-driver`, `sdtm-sv-subject-visits`, `negative-input-require-match`.
Replaces: REQ-0111, REQ-0150, REQ-0151, REQ-0152, REQ-1051, REQ-1052.

```yaml
input:
  ADSL: {path: adsl.csv, require_match: true}
  PERIODS: periods.csv
columns:
  - name: TRTSDTM
    derivation: ADSL.TRTSDTM
  - name: APERIOD
    derivation:
      source:
        variable: PERIODS.APERIOD
        between: {value: ASTDT, lower: APERSDT, upper: APEREDT}
```

The driver's qualifier reads the input or intermediate record that built the
row. A `between` value is not a scalar source and never joins: it reads the
current row alone ([LOOKUP-9](#lookup-9)). An applicable key compares its two
sides as the match step does, and [VALUE-2](values.md#value-2) performs no
implicit conversion: a key whose left and right types are not mutually
comparable fails as `incompatible_input_type`, reporting both types
([LOOKUP-9](#lookup-9)). With no applicable key the
intended match is unclear, and the read fails as `no_applicable_keys`
([LOOKUP-5](#lookup-5)). The author then states the match with a named
intermediate declaring its `key`, the same explicit form that serves whenever
the intended keys differ from the applicable keys.

A structured `source:` binding, or a filtered source that carries a variable
with its range, filter, selection or absent value, keeps them on the implicit
join exactly as a named intermediate would: `between` keeps the matched
records whose bounds hold the current row's value ([LOOKUP-9](#lookup-9)), the
filter narrows them ([LOOKUP-8](#lookup-8)), and the ordered selection chooses
among the survivors ([LOOKUP-10](#lookup-10)). Above, each row takes the
period whose first and last days hold its own `ASTDT`, an output column; in a
row template the value may instead read the template's driver record
([LOOKUP-2](#lookup-2)).

A qualified read that is not a source binding, such as an operation's
variable operand or an identifier in a predicate, declares no absence handler
and gives missing for a row with no match, counted at its path except in a
predicate ([LOOKUP-24](#lookup-24)). `require_match: true` on the input covers
every one of these reads.

<a id="lookup-2"></a>

### LOOKUP-2. Row construction reads another dataset through the same join, using only row-phase values

During ungrouped row construction, a scalar source qualified with another
input dataset joins that dataset on the applicable keys, each matching the
same-named field of the row template's input record. During grouped row
construction each applicable key instead matches the candidate's group-key
value, and every applicable key must be a group key of the row template. A
match value must be available in the row phase; one available only in a later
phase fails.

Fails: `ungrouped_driver_field`, `unknown_field`, `phase_boundary`.
Evidence: `negative-grouped-lookup-ungrouped-key`, `negative-grouped-range-ungrouped-value`, `negative-lookup-key-column-phase`, `negative-lookup-match-column-phase`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs`, `adam-adlb-supplb-padded-key`, `schema-lookup-source-range-driver`, `rust/crates/yamaa-adapters/tests/execution_lookup_model.rs`, `rust/crates/yamaa-adapters/tests/execution_lookup_read_contexts.rs`, `rust/crates/yamaa-adapters/tests/execution_row_phase_reads.rs`.
Replaces: REQ-0126, REQ-0156, REQ-0157.

In ungrouped row construction each retained input record builds one candidate
row ([ROW-3](rows.md#row-3)). The input record's fields are
the only single values the match can read; values produced only by column
derivation are not available then. A named intermediate states the same match
with declared `key` pairs, and its match values have the same availability:
input-record fields or columns of the same row template. The join binds one
value per row, and later derivations in the same row template read that value
through the bound column. A row-phase `compute` still names no qualified
cross-dataset identifier ([FORMULA-9](formulas.md#formula-9)).

In grouped row construction every applicable key must be a group key of the
row template ([ROW-4](rows.md#row-4)). A key the group does not
carry varies within the group and names no single match value, and fails as
`ungrouped_driver_field` ([ROW-5](rows.md#row-5)). A key no
input record carries fails as `unknown_field`
([NAME-3](names.md#name-3)), in either mode.

A `between` value has the same availability as a match value
([LOOKUP-9](#lookup-9)): during ungrouped row construction it reads a field of
the template's driver record, qualified by the driver, or a column the same
template derives; during grouped row construction a driver field it reads must
be a group key of the template, or it fails as `ungrouped_driver_field`.
During grouped row construction a named intermediate's key match value, and a
derivation of the intermediate that reads the current record
([LOOKUP-15](#lookup-15)), fail the same way when they read a driver field
outside the group keys. Each finding is at that value, names the field, the
intermediate, the row template and its driver, and is reported once whichever
reads of the intermediate reach it.

During grouped row construction, every variable a lookup matches on must be
derived by the row template that reads the lookup. The exceptions are the
template's group keys and, for an ungrouped template, the driver record's own
fields.

In either mode, [Column derivation](columns.md) orders row derivations before
column derivation, and a match value available only in a later phase fails as
`phase_boundary`. A named intermediate declares its match once for every read,
so the finding is at the row template's read of the intermediate, in a
derivation or the row `filter`, once for that site. It names the value, the
intermediate and the row template, whether the value is a `key` match value,
the intermediate's `between` value, or a match value of another intermediate
that a `between` value reads; in that last case the finding also names each
value followed, from the first `between` value to the value that reads the
column. A structured source's own `between` value is written in the row
derivation, and a later-phase one fails as that derivation's read does
([COLUMN-4](columns.md#column-4)), at `between.value`.

## Named intermediates

<a id="lookup-3"></a>

### LOOKUP-3. A named intermediate declares an id and a dataset, and an omitted key is the applicable keys

An entry of `intermediates` declares `id` and `dataset`, the relation it
reads; `key` is optional. Its `id` shares one naming list with dataset
identifiers, other lookup ids and the output `domain`, and a collision fails.
An omitted `key` is the applicable keys ([LOOKUP-5](#lookup-5)), and a `key`
is stated only when the intended match differs from that inferred match.

Fails: `duplicate_identifier`, `missing_required_field`.
Evidence: `negative-lookup-id-collision`, `negative-lookup-unknown-key`, `schema-lookup`.
Replaces: REQ-0112, REQ-0113, REQ-0114, REQ-0153, REQ-1048, REQ-1050.

For a named intermediate, `dataset: SELF` instead reads the specification's
completed derived rows ([LOOKUP-7](#lookup-7)). The current row is the output
row, or grouped-row candidate, the lookup runs for. The match fields are the
`key` columns, the match values are what the current row supplies for them
([LOOKUP-5](#lookup-5)), and the candidates are the matched records that
survive `filter` ([LOOKUP-25](#lookup-25)).

An `id` collision fails as `duplicate_identifier`. The schema requires `id`
and `dataset`: omitting either fails as `missing_required_field` with no
requirement attached. An omitted `key` with no applicable key fails as
`no_applicable_keys` ([LOOKUP-5](#lookup-5)).

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

The lookup above matches on the applicable output keys. The form below states
the same match explicitly:

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

<a id="lookup-4"></a>

### LOOKUP-4. A named intermediate must narrow, derive or reshape its dataset

A named intermediate must narrow, derive or reshape its dataset, or declare
`require_match: true`. An intermediate that declares nothing beyond `id` and
`dataset`, with or without `no_match: {literal: null}`, reads exactly what the
implicit join reads ([LOOKUP-1](#lookup-1)) and fails: read the input dataset
directly instead of aliasing it. Composition preserves this refusal even when
no output reads the declaration.

Fails: `rename_only_intermediate`.
Evidence: `negative-lookup-rename-intermediate`, `sdtm-cm-whodrug-coding`.
Replaces: REQ-1248.

```yaml
# rejected: CODED adds nothing to CODING
intermediates:
  - id: CODED
    dataset: CODING
    no_match: {literal: null}
```

Without `no_match` the intermediate answers a row without a record with
missing as well ([LOOKUP-24](#lookup-24)), so omitting the line changes
nothing. Asserting a match is a change the implicit join cannot state:
`require_match: true` makes the same declaration valid.

Instead, qualify the dataset where it is read:

```yaml
intermediates:
  - id: DRUG
    dataset: WHODRUG
    key:
      DRUG_RECORD_NO: CODING.DRUG_RECORD_NO
      ATC_CODE: CODING.ATC_CODE
    no_match: {literal: null}
```

An implicit join to `CODING` also needs matching output-key types. When the
output's `CMSEQ` is numeric but `CODING.CMSEQ` is stored as text, declare
`CODING.CMSEQ` as `int` on input before reading `CODING.DRUG_RECORD_NO`.

<a id="lookup-7"></a>

### LOOKUP-7. A named intermediate may read SELF, the completed rows of earlier row templates

A lookup's `dataset` must be declared in `input`, except that a named
intermediate may use `SELF` when the specification declares row templates.
`SELF` holds the rows of completed earlier templates during row construction
and all completed rows during column derivation; the current template's
unfinished rows are never eligible, and a read from the first template fails.
`SELF` is reserved for this purpose, cannot also name an input, and is read
only through a named intermediate, never as a directly qualified source.

Fails: `phase_boundary`.
Evidence: `adam-adae-prior-serious-event`, `adam-advs-windows`, `negative-self-window-first-template`, `negative-self-filter-first-template`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs`.
Replaces: REQ-0120.

A read from the first template fails as `phase_boundary`. So does a
first-template read of another named intermediate whose `between` value reads
a `SELF` intermediate, directly or through the `between` values of further
intermediates; the finding names the read, then each `between` value in the
order followed. The first template's row `filter` reads for that template's
candidates, so a read there fails the same way, at `rows[0].filter`; a
grouped template's `filter` reads no qualified name at all
([ROW-6](rows.md#row-6)). The donor fields are output columns derived in every
row template; a column derived only in the later column phase is unavailable
to `SELF`. The order of donors follows row construction order.

For `SELF`, `filter` and `order_by` may name its donor fields either bare or as
`SELF.field`. Bare donor fields in `filter` are not correlated current-row
references. All other lookup rules, including matching, missing values,
ordering and `columns`, apply to `SELF` as they do to input datasets; the
filter scopes of an input dataset are in [LOOKUP-8](#lookup-8).

A `key` match value still reads the current row ([LOOKUP-5](#lookup-5)): a
bare name there is the current row's column, not a donor field. Row templates
reading one `SELF` intermediate can each supply a match value from their own
derivations. Below, an expected-record template derives its window and reads
`COVER` to find a completed row already in that window; another template
deriving `AVISIT: {literal: WEEK 4}` reads the same intermediate for its own
window.

```yaml
intermediates:
  - id: COVER
    dataset: SELF
    key: {USUBJID: VS.USUBJID, PARAMCD: VS.VSTESTCD, AVISIT: AVISIT}
    order_by: [SELF.VSSEQ]
    keep: last
    no_match: {literal: null}
rows:
  # ...after the template that builds the collected rows
  - id: expected_week2
    dataset: VS
    group_by: [VS.USUBJID, VS.VSTESTCD]
    filter: "COVERED IS NULL"
    derivations:
      AVISIT: {literal: WEEK 2}
      COVERED: COVER.VSSEQ
```

<a id="lookup-11"></a>

### LOOKUP-11. A lookup-qualified variable reads a column of the selected record

A variable qualified by a lookup id, such as `DEATHEV.AEDECOD`, reads the named
column of the selected record in any field typed as `variable`. A declared
`columns` list restricts which columns the lookup may read; without it every
stored and derived column is available. A column the lookup's dataset does not
have, or one outside a declared `columns` list, fails.

Fails: `unknown_field`.
Evidence: `adam-adae-death`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs`.
Replaces: REQ-0122, REQ-0125.

Naming in `columns` a column the dataset does not have fails as
`unknown_field`, and so does a lookup-qualified read of a missing or
unlisted column.

<a id="lookup-12"></a>

### LOOKUP-12. A lookup matches once per row when first read and never changes the row count

A named lookup matches only when it is read. The match runs the first time a
column reads the lookup for a row, and every later read in that row shares the
selection, so every column reading the same lookup sees the same record. A
lookup answers one value per current row, the selected record's column, its
absence answer or an `unmatched_key` failure under `require_match`, and never
changes the row count. An input lookup's dataset is read once per run.

Evidence: `rust/crates/yamaa-adapters/tests/execution_key_lookup_reads.rs`, `rust/crates/yamaa-adapters/tests/execution_per_template_defaults.rs`.
Replaces: REQ-0136, REQ-0138, REQ-0146, REQ-1264.

The per-row match selects among records already read and never re-reads the
dataset. An intermediate declaring `require_match: true` never fails as
`unmatched_key` on a row that did not read it. A `SELF` intermediate extends
its donor pool after each completed row template and invalidates its cached
selection index at that boundary.

## Intermediate derivations

<a id="lookup-15"></a>

### LOOKUP-15. Intermediate derivations add per-record fields to the donor records before matching

An intermediate may declare `derivations:`, a map of names to derivations in
the expression language of row-template `derivations:`, evaluated in
declaration order over the dataset's stored fields and the derivations
declared before each one. A typed entry converts each result to its declared
type before anything later reads it, and an untyped entry keeps its raw
result, its field taking the expression's result type
([EXPR-14](expressions.md#expr-14)). The derived values augment the donor
records before matching, `filter`, `order_by` selection and `columns`
projection, and behave there like stored fields; derivations that read the
current input record augment only the records that match, as below.

Fails: `unknown_field`, `duplicate_derivation`.
Evidence: `schema-lookup-intermediate-derived-filter`, `schema-lookup-intermediate-correlated-derive`, `schema-lookup-intermediate-closest-datetime`, `schema-window-intermediate-rank`, `negative-lookup-derivation-scope`, `rust/crates/yamaa-adapters/tests/execution_lookup_read_contexts.rs`, `rust/crates/yamaa-adapters/tests/execution_lookup_matched_derivations.rs`.
Replaces: REQ-1185.

Each entry is either an untyped expression, including its
`{value: <expression>}` wrapper, or a typed binding
`{type: <column_type>, derivation: <expression>, unconvertible: <literal>?}`.
An untyped entry cannot declare `unconvertible`, and an untyped handler
wrapper fails schema validation. A typed entry converts each result under
[VALUE-3](values.md#value-3) before any later derivation, filter,
match, ordering or verification reads it. Missing stays missing. A conversion
failure is fatal without `unconvertible`; a declared literal replacement
converts to the same type and is counted under
[COLUMN-13](columns.md#column-13). The conversion boundary is the
binding's result; expressions still never convert implicitly
([VALUE-2](values.md#value-2)). For example:

```yaml
derivations:
  VISITN:
    type: int
    derivation: {str_extract: {source: VISIT, pattern: '^WEEK ([0-9]+)$', group: 1}}
    unconvertible: {literal: null}
```

`WEEK 02` thus yields the integer `2` before numeric matching. `check` reports
incompatible key types under [VALUE-2](values.md#value-2) when the
declared source types and the derivations' result types establish the
mismatch without reading data, and a typed entry whose result type can never
convert to its declared type fails under [VALUE-24](values.md#value-24).

A bare name reads the dataset's stored field or an earlier derived name. A
qualified name may read the intermediate's dataset, the current input record
under the same scope as a correlated `filter` ([LOOKUP-8](#lookup-8)), or
another named intermediate under [LOOKUP-16](#lookup-16). A reference to an
unrelated input dataset, a derivation declared later in the same map, or an
unknown field fails as `unknown_field`; a derived name that shadows a stored
column fails as `duplicate_derivation`. A current-record read is checked in
every context that reads the intermediate: when a row reading it is built from
another input's record, the read names no value of that row and fails as
`unknown_field` at the derivation, once. A project function that a derivation
calls is invoked once per donor record, so an argument that reads a named
intermediate reads only one that declares `keep` ([LOOKUP-22](#lookup-22));
an argument may read the donor record's own fields as any operand does.

Derivations that read only donor records are computed once per record for
each build of the donor records; [COLUMN-14](columns.md#column-14) lists when
the run builds them, which its handler counts follow. When any derivation
reads the current input record, the map evaluates for each lookup's current
record, after that record's match values, and only on the donor records whose
stored keys equal them, in stored order; the range, the filter and the
selection see those records. A record that no current record's match values
select is never augmented, so a derivation cannot fail on it and no handler
counts it. When a `key` or a `between` bound names a derived field, or a
derivation uses a window function, the map evaluates on every donor record
instead. The augmented donor records and their filters are not reused for
another current record. Every column reading the intermediate still shares
the same selection ([LOOKUP-12](#lookup-12)).

A derivation may use a window function. The window partitions the donor
records as augmented by every derivation declared before it, so its
`group_by`, `order_by` and `filter` may read a stored field or an earlier
derived name, bare or dataset-qualified. A window's fields cannot read the
current input record directly: derive that value first and let the window read
the derived name. Base-record order is the final tie-break.

A derived name may appear as a target-side `key`, as a bare `between` bound, as
a dataset-qualified field in `filter` or `order_by`, in `columns`, or in
`verification.unique`, and behaves like a stored field there. A correlated
filter sees the augmented donor record and the current driver record together.
A derivation that fails on a record fails the run with the expression's
condition at the derivation's path. A derivation that yields missing
contributes an ordinary missing value, which cannot match a key and may be
excluded by a predicate.

```yaml
intermediates:
  - id: EOT
    dataset: DS
    derivations:
      EOT_FALLBACK:
        compute:
          expr: "DSSTDY + DSSEQ / 1000"
    filter: "DS.EOT_FALLBACK IS NOT NULL"
    order_by: [DS.EOT_FALLBACK]
    keep: first
    columns: [DSDECOD, EOT_FALLBACK]
```

For a measurement closest to a reference on the current input record:

```yaml
base: BASE
intermediates:
  - id: PICK
    dataset: SRC
    key: [ID]
    derivations:
      DIST: {compute: {expr: "ABS(DAY - BASE.REF)"}}
    filter: "SRC.DIST IS NOT NULL"
    order_by: [SRC.DIST]
    keep: first
    columns: [VALUE, DIST]
    no_match: {literal: null}
```

The `filter` drops every `SRC` record whose distance is missing. When the
current input record's reference is missing, every distance is missing, no
candidate survives, and the read takes `no_match` ([LOOKUP-24](#lookup-24)).

`ABS(DAY - BASE.REF)` is the number of days between two days only on a
numbering without a gap. A study day (`--DY`, `ADY`) has no day 0
([TIME-6](temporal.md#time-6)), so when one study day falls before day 1 and
the other on or after it, their difference is one more than the days between
their dates: day -1 and day 1 are consecutive dates, yet the difference gives
2, so a candidate on day -1 loses to one on day 2, which is no nearer a
reference on day 1. Measure such a distance between dates instead, here the
candidate's `ADT` and the reference's `BASE.REFDT`, with `diff`
([TIME-4](temporal.md#time-4)):

```yaml
    derivations:
      GAP: {diff: {type: date, start: ADT, end: BASE.REFDT, unit: day}}
      DIST: {compute: {expr: "ABS(GAP)"}}
```

To fall back to a candidate instead, leave the filter out and place missing
distances last. Without `nulls`, a missing distance sorts first under `asc`
([VALUE-22](values.md#value-22)), and a candidate without a distance is
selected ahead of every measured one. Remaining ties break by record order
([LOOKUP-10](#lookup-10)), which is not a business key
([INPUT-2](inputs.md#input-2)), so the fallback names its tie-break:

```yaml
    order_by: [{variable: SRC.DIST, nulls: last}, SRC.SEQ]
```

A measured candidate is then selected ahead of every candidate without a
distance. A selected record whose `DIST` is missing shows that no candidate had
a distance and the tie-break chose it.

`ABS` drops the direction, so a candidate before the reference and one after
it at the same distance tie, and record order chooses between them
([LOOKUP-10](#lookup-10)). Order next on the signed `GAP`, which is positive
before the reference, and then on a sequence number:

```yaml
    order_by: [SRC.DIST, {variable: SRC.GAP, direction: desc}, SRC.SEQ]
```

`direction: desc` prefers the candidate before the reference, and `asc` the
one after it ([VALUE-22](values.md#value-22)); `SEQ` decides between
candidates that still tie, such as two records at the same time. For a
baseline-type selection, which reads only records on or before the reference,
filter on `SRC.GAP >= 0` rather than taking the nearest record on either
side; a missing `GAP` never satisfies it ([LOOKUP-8](#lookup-8)).

When the reference is a `datetime`, such as `BASE.REFDTM`, `impute` completes
each candidate's collected text ([TIME-8](temporal.md#time-8)), `diff` takes
the elapsed difference ([TIME-4](temporal.md#time-4)), and `ABS` makes it a
distance:

```yaml
    derivations:
      ADTM: {impute: {type: datetime, source: DTC, minimum_source_precision: day, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: BASE.REFDTM, unit: hour}}
      DIST: {compute: {expr: "ABS(GAP)"}}
```

`diff` with `type: datetime` reads two `datetime` values
([TIME-1](temporal.md#time-1)), so the reference must be one: a field its
input declares `datetime`, which must hold complete text
([VALUE-15](values.md#value-15)), or a value `impute` completed. A reference
collected as text that may be partial or have no time, such as an SDTM
`--DTC`, takes its own `impute` in the same derivations, and the difference
reads the completed value:

```yaml
      REFDTM: {impute: {type: datetime, source: BASE.REFDTC, minimum_source_precision: day, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: REFDTM, unit: hour}}
```

`GAP`, and with it `DIST`, is missing when `ADTM` or the reference is missing,
including a reference whose text is coarser than its minimum precision. A
candidate without a distance is dropped by the filter, or placed last by
`nulls: last` when the filter is left out. For each candidate's `DTC`:

- A missing `DTC` gives a missing `ADTM` without any handler
  ([TIME-13](temporal.md#time-13)).
- Partial collected text such as `2024-03` is valid
  ([TIME-7](temporal.md#time-7)), so `invalid` never answers it. Under
  `minimum_source_precision: day` it gives a missing `ADTM` and has no
  distance ([TIME-8](temporal.md#time-8)). A coarser minimum completes it
  from the rules that minimum requires ([TIME-10](temporal.md#time-10)):
  with `minimum_source_precision: month` and `day: first`, `2024-03` becomes
  `2024-03-01T00:00:00`, and the distance is measured from that imputed
  value. To measure in days instead, complete a `date` with `impute` of
  `type: date`, which takes no `time`, and take the day difference with
  `diff` of `type: date`, as for study days above: `2024-03` then becomes
  `2024-03-01`.
- A complete date without a time, such as `2024-03-10`, takes its time from
  `time`: `2024-03-10T00:00:00` under `time: first`.
- Text that is not collected date text, such as `2024-03-10 09:00`, fails as
  `invalid_date_text` at the derivation's path ([TIME-13](temporal.md#time-13)).
  One such record in `SRC` stops the run, even when no current input record
  matches its key, unless `invalid: {literal: null}` answers it with a missing
  `ADTM` ([COLUMN-10](columns.md#column-10)).

`time: first` is not neutral for a distance. Against a 09:00 reference, a
date-only candidate on the same day is imputed to 00:00, 9 hours away, and a
14:00 candidate, 5 hours away, wins; `time: last` moves the date-only
candidate to 23:59:59 instead. How a missing time, day or month is completed,
for a candidate or the reference, comes from the statistical analysis plan.
When the imputed datetime is published, publish its imputation flags beside
it: derive `ATMF: {imputation_flag: {source: ADTM, part: time}}` after `ADTM`
([TIME-18](temporal.md#time-18)), and a `part: date` flag as well when a
coarser minimum supplies a day or month, and list them in `columns`. `ATMF`
is `H` for a date-only candidate, and `seconds_collected: false` keeps a study
that collects times to the minute from flagging every timed candidate `S`.

To exclude the candidates collected without a time instead, read the
precision of each candidate's text ([TIME-7](temporal.md#time-7)) and keep
those collected to the minute or the second:

```yaml
    derivations:
      PREC: {precision: {type: datetime, source: DTC}}
      ADTM: {impute: {type: datetime, source: DTC, minimum_source_precision: day, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: BASE.REFDTM, unit: hour}}
      DIST: {compute: {expr: "ABS(GAP)"}}
    filter: "SRC.DIST IS NOT NULL AND SRC.PREC IN ('minute', 'second')"
```

<a id="lookup-16"></a>

### LOOKUP-16. An intermediate derivation may read another intermediate's selection for its donor record

An intermediate derivation may read a column of another named intermediate
through that intermediate's id. The read runs the other intermediate's match
for the donor record being augmented, not for an output row: every match
value, `between` value and correlated `filter` field the match reads comes from
that donor record, written bare or qualified by the reading intermediate's
dataset. Matching, range narrowing, filtering, ordered selection and the
absence policy apply as for any other read ([LOOKUP-25](#lookup-25)), once per
donor record.

Fails: `unknown_field`, `phase_boundary`, `dependency_cycle`.
Evidence: `adam-adlb-end-of-treatment`, `adam-adsl-crossover-sequence`, `rust/crates/yamaa-adapters/tests/execution_aggregate_record_operations.rs`, `rust/crates/yamaa-adapters/tests/project_environment.rs::intermediate_callback_reads_a_kept_selection_beside_its_own_donor_field`.
Replaces: REQ-1263.

Either form of a donor-record name names a stored field or a derivation
declared before the reading one, and a name the donor record cannot supply
fails as `unknown_field`. The read column must exist in the other
intermediate's dataset or derivations and, when that intermediate declares
`columns`, be one of them ([LOOKUP-11](#lookup-11)).

The other intermediate is selected once per donor record, and every derivation
of that record that reads it shares the selection ([LOOKUP-12](#lookup-12)). A
read that fails, such as an intermediate declaring `require_match: true` that
selects nothing, fails the run with that intermediate's condition at that
intermediate's path, as a row reading it would. The read adds no current-row
dependency and never changes the number of donor records. Any operand may read
an intermediate without `keep`, which fails as `multiple_matches` when it
matches several records for a donor record ([LOOKUP-10](#lookup-10)), except a
project function argument, which may read only an intermediate that declares
`keep` ([LOOKUP-22](#lookup-22)).

The other intermediate must read an `input` dataset. A `SELF` intermediate has
no completed rows while donor records are augmented, and reading one fails as
`phase_boundary`. Intermediates whose derivations read each other, directly or
through a chain, fail as `dependency_cycle`. A window's fields read only the
donor records being partitioned: a read of another intermediate in a window's
`group_by`, `order_by`, `filter` or value field fails as `unknown_field`.
Derive the value first and let the window read the derived name.

```yaml
intermediates:
  - id: SUPP_EP
    dataset: SUPPLB
    filter: "SUPPLB.QNAM = 'ENDPOINT'"
    key:
      STUDYID: LB.STUDYID
      USUBJID: LB.USUBJID
      IDVARVAL: {str_pad: {source: LB.LBSEQ, width: 8}}
  - id: EOT_RANK
    dataset: LB
    key: [STUDYID, USUBJID, LBSEQ]
    derivations:
      ENDPOINT: SUPP_EP.QVAL
      EOT_SEQ:
        row_number:
          window:
            group_by: [LB.USUBJID, LB.LBTESTCD]
            order_by:
              - {variable: ENDPOINT, direction: desc}
              - {variable: LB.VISITNUM, direction: desc}
```

Each laboratory record reads its own supplemental endpoint qualifier, and the
window ranks the records of each subject and test on it.

<a id="lookup-17"></a>

### LOOKUP-17. An intermediate's unique verification asserts that no donor column combination repeats

An intermediate may declare `verifications:`, a list of `unique` checks, each
asserting that its non-empty column combination is unique across the
intermediate's source-only filtered donor records with `derivations:`
computed. Input-backed checks run before any row is built, and `SELF` checks
run after each completed template. A repeated combination fails the run, and
there is no warning severity. A lookup whose key is verified unique needs
neither `keep` nor `order_by`.

Fails: `duplicate_intermediate_records`, `unknown_field`, `correlated_derivation_with_unique_verification`.
Evidence: `schema-lookup-intermediate-unique`, `negative-lookup-duplicate-intermediate`.
Replaces: REQ-1245.

```yaml
- id: DS_EOS
  dataset: DS
  filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.DSDECOD <> 'SCREEN FAILURE'"
  verifications:
    - unique: [STUDYID, USUBJID]
```

The mapping form `unique: {columns: [STUDYID, USUBJID], id: donor-key}` names a
check when an explicit ID is useful, and both forms test the same donor
records; a mapping holding only `columns` restates the list and fails as
`redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15). Each check may declare an
optional `id`, governed by
[CHECK-3](verification.md#check-3). [LOOKUP-10](#lookup-10) already rejects
multiple surviving matches without a selection rule.

A repeated combination fails as `duplicate_intermediate_records` at the
`intermediates[i].verifications[j].unique` path. Each `unique` column must name
a stored field or a derived name, or fail as `unknown_field`. A `filter` or
`derivations:` entry that fails to materialize fails the run here as well. A
`filter` that references the current input record is evaluated per record and
cannot combine with `verifications:`; the combination fails validation. A
derivation that reads the current input record likewise cannot combine with
`verifications:` and fails as
`correlated_derivation_with_unique_verification`. `keep` stays a positional
selection requiring `order_by`, and an unordered `keep: first` is not an
exactly-one assertion.

## Aggregates over a relation

<a id="lookup-18"></a>

### LOOKUP-18. An aggregate reduces one declared dataset, the constructed output rows, or the current input group

Every identifier in one aggregate expression names one relation, in exactly
one of three contexts. Qualified: every identifier names the same declared
dataset. Unqualified: every identifier names a constructed output column, and
the result is broadcast to every row of its `group_by` partition. Grouped
input: every identifier is qualified to the input dataset of the enclosing
grouped row template, and the expression reduces the current input group to
one candidate-row value. Every other context fails, and the forms must not be
mixed.

Evidence: `schema-aggregation`, `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-adapters/tests/fixtures/row_reductions.json`.
Replaces: REQ-0296, REQ-0329, REQ-0465, REQ-0466, REQ-0467, REQ-0468,
REQ-0469, REQ-0504, REQ-0506, REQ-1088.

An identifier is `NAME` or `DATASET.NAME`, which
[Name binding](names.md) resolves in the same phase; a
reducer expression and a predicate resolve each name the same way. An
aggregate reduces records from one relation, returns one value per group, and
never changes the row count.

- **Qualified.** During column derivation the expression reduces the
  candidates of its lookup: the right-side records its key pairs and range
  match and its filter keeps ([LOOKUP-25](#lookup-25)), even when the
  expression qualifier equals the current row template's input dataset. A
  scalar source qualified to that input dataset reads one record; the
  aggregate keyword makes the same qualifier relational.
- **Unqualified.** The expression reduces constructed output rows within its
  `group_by` partition and broadcasts the result to every row in the
  partition.
- **Grouped input.** The expression is a row derivation that reduces only the
  records of the current input group. The grouped row template asks it for one
  value while [Column derivation](columns.md) decides whether
  that candidate row is appended.

A single expression naming two datasets, or mixing a qualified identifier with
an unqualified one, fails. A reduction is not a join: an expression combining
two dataset relations first binds each relation to a column and then combines
the results with `compute`, which admits a qualified identifier only for a
record selected by a named intermediate and still rejects an arbitrary
dataset-qualified identifier ([FORMULA-9](formulas.md#formula-9)). Every join
remains under these lookup rules.

A qualified name in an aggregate expression reaches a field of its relation,
never an ODM item by its `ItemOID`, and a qualified name that names an ODM item
fails. ODM item identifiers carry further periods; read the item into a column
with [`odm`](names.md#name-7) first.

The context decides what `group_by` ([LOOKUP-19](#lookup-19)), `key`
([LOOKUP-5](#lookup-5)), `between` ([LOOKUP-9](#lookup-9)), `filter`
([LOOKUP-8](#lookup-8)) and `derive` ([LOOKUP-22](#lookup-22)) range over;
`expr` is the reducer expression of [FORMULA-17](formulas.md#formula-17).

<a id="lookup-19"></a>

### LOOKUP-19. A qualified aggregate groups by output keys and an unqualified one by output columns

A qualified aggregate's `group_by` declares qualified right-side columns, and
each must also be an output key; the reduction stays coarser than or equal to
the applicable keys it joins on. An unqualified aggregate's `group_by`
declares constructed output columns and must declare at least one; a reduction
over the whole output is not registered. A grouped-row aggregate declares no
local `group_by`: the enclosing `row.group_by` already fixes its current
relation and keys.

Evidence: `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-core/tests/schema_windows.rs`.
Replaces: REQ-0331, REQ-0470, REQ-0507, REQ-0508.

A qualified `group_by` column that is not an output key fails, as does an
unqualified expression with no `group_by` or with a `between`. A grouped-row
aggregate fails when it declares its own `group_by`, declares `key`
([LOOKUP-5](#lookup-5)) or `between` ([LOOKUP-9](#lookup-9)), names an
identifier outside its row template's input dataset, or is used by an
ungrouped row template.

<a id="lookup-22"></a>

### LOOKUP-22. A qualified aggregate's derive bindings compute typed per-record values before reduction

A qualified aggregate may declare `derive` to bind per-record intermediate
variables before reduction. Each binding declares a `name`, a `type` and a
`derivation`; bindings evaluate once per record of the aggregate's relation,
in declaration order, and each derivation reads the record's fields and the
values bound by earlier bindings. The reducer expression names each bound
variable by its unqualified `name`. A binding cannot use a window operation,
and `derive` is not available on unqualified or grouped-input aggregates.

Fails: `invalid_derive_step`.
Evidence: `schema-derive-text-score`, `schema-derive-date-epoch`, `schema-aggregate-derive-intermediate`, `negative-donor-function-unkept`, `rust/crates/yamaa-core/tests/support/diagnostic/project_functions.rs::donor_keep_reached`.
Replaces: REQ-1189, REQ-1190, REQ-1191, REQ-1192, REQ-1242.

A derive binding is not an expression function. Text becomes a number only
through the binding's declared `type`, and a date becomes an integer only
through an operation such as `to_epoch_day` in the binding's own derivation;
an `aggregate` or `compute` expression never parses text or dates itself. Each
binding's evaluated value converts to its declared `type` under
[VALUE-3](values.md#value-3), and a binding whose result type
([EXPR-14](expressions.md#expr-14)) can never convert to it fails validation
under [VALUE-24](values.md#value-24). Text that is not convertible to the
declared numeric type fails as a conversion failure under
[VALUE-3](values.md#value-3), answered by a declared `unconvertible`
handler and fatal otherwise. A missing value stays missing without attempting
conversion.

The window operations are `row_number`, `rank`, `row_value`, `locf` and
`baseline_flag`, including inside `case`; validation reports
`invalid_derive_step` at the window's path.

The relation a derived aggregate reduces is the one relation its derive
bindings and filter name, and naming two relations is an error. A reducer
expression that mixes a bound variable with a qualified identifier is an error
under [LOOKUP-18](#lookup-18). Qualifiers naming declared named intermediates
do not count as relations for this rule. A binding may read a named
intermediate that declares `keep`: with `keep` the intermediate selects
exactly one record per row ([LOOKUP-10](#lookup-10)), and the read is a
row-scoped value, not another reduced relation. The binding still evaluates
once per record of the aggregate's one relation, and each record sees the same
intermediate value for its row. A binding that reads a named intermediate
without `keep` is an error.

A project function called in an intermediate derivation
([LOOKUP-15](#lookup-15)) is invoked once per donor record, so its arguments
follow the same rule: an argument may read a named intermediate that declares
`keep`, and an argument that reads one without `keep` fails as
`invalid_derive_step` at that argument, naming the function, the argument and
the intermediate.

For ordered pairwise reductions, first construct one row per sample and derive
its neighbours with [window operations](windows.md), then derive each segment
as an ordinary column. A second specification reduces the stored segments at
the subject grain. Its input may declare
`SEGS: {path: segments.csv, schema: segments.yaml}` under
[INPUT-10](inputs.md#input-10); the caller builds and saves the
producing specification before building the consumer, which reads the saved
artifact. For the linear trapezoid, the
producer's segment expression is `0.5 * (PREVC + AVAL) * (ARELTM - PREVT)`,
and the consumer's reduction is `SUM(SEGS.SEG)`. A first sample's missing
neighbour produces a missing segment, which `SUM` skips under
[FORMULA-19](formulas.md#formula-19).
