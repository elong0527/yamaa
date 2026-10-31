# Lookup and joins

A derivation reads another relation through the implicit join of a
dataset-qualified source, through a named intermediate declared in
`intermediates`, or through a qualified aggregate. Each is one lookup
([LOOKUP-25](#lookup-25)): it matches records on keys and an optional range,
narrows them by a filter, selects one record or reduces them, and answers
absence. The kinds of lookup differ only in the steps they declare. A named
intermediate that writes `inner_join` is a fourth kind, the pair relation: it
matches and narrows the records of one relation for every record of another
and keeps every pair ([LOOKUP-27](#lookup-27)).

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

Evidence: `schema-lookup-source-range`, `schema-lookup-filters`, `schema-aggregation`, `schema-lookup-inner-join-pairs`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`.

| Step | Implicit join | Named intermediate | Qualified aggregate | Pair relation |
| --- | --- | --- | --- | --- |
| Match keys | The input's `key` ([LOOKUP-1](#lookup-1)) | `key` ([LOOKUP-3](#lookup-3)) | `key`, or the input's `key` ([LOOKUP-5](#lookup-5)) | `key`, the left record's values against the right side's fields ([LOOKUP-27](#lookup-27)) |
| Match range | `between` on a structured `source:` | `between` | `between` | None |
| Narrow | `filter` on a structured `source:`, over the donor record | `filter`, which may also read the current row's driver | `filter`, over the donor record | `filter`, over both sides' records |
| Select | `order_by` and `keep` | `order_by` and `keep`, or `nearest` ([LOOKUP-26](#lookup-26)) | The reduction of `expr` over every candidate ([LOOKUP-18](#lookup-18)) | None: every candidate makes one pair |
| Answer absence | `absent`, missing by default, or `require_match` | `absent`, missing by default, or `require_match` | Missing, uncounted ([FORMULA-24](formulas.md#formula-24)) | None: a record without a candidate is in no pair |

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

Each kind adds only what it needs. An implicit join declares no `key` of its
own and matches on its input's ([LOOKUP-1](#lookup-1)). A named intermediate is
read through its id, and every column reading it for a row shares one selection
([LOOKUP-12](#lookup-12)). An aggregate reduces every candidate instead of
selecting one, so it declares no `keep`, and an `order_by` only to order the
values `STR_JOIN` joins ([LOOKUP-10](#lookup-10)). An unqualified aggregate
and a grouped-row aggregate are not lookups: they reduce the constructed output
rows of a partition or the current input group, which their `filter` narrows
([LOOKUP-8](#lookup-8)), and declare no `between` ([LOOKUP-19](#lookup-19)). A
pair relation runs the match and narrow steps once for every record of its
left side, that record standing where the current row stands, and keeps every
candidate as a record of its own; it runs once, before any row is built
([LOOKUP-27](#lookup-27)).

<a id="lookup-9"></a>

### LOOKUP-9. Match keeps the records whose keys equal the current row's match values and whose bounds hold its range value

Step one keeps records whose keys equal their paired match values and whose
`between` bounds hold the range value.
`closed` chooses which endpoints are included: `both` (the default),
`left` (lower only), `right` (upper only) or `neither`. Values compare exactly
under [VALUE-19](values.md#value-19), with no operand converted, and a pair or
bound whose types cannot compare fails. A missing match or range value seeks
no record, and a record missing a key or a stated bound does not match.
`between` names a current-row value and at least one bound field.

Fails: `incompatible_input_type`, `incomparable_range_types`, `invalid_field_type`, `prohibited_construct`, `unknown_field`.
Evidence: `negative-lookup-bad-range`, `negative-lookup-range-other-input`, `negative-lookup-range-bare-field`, `negative-source-range-other-input`, `negative-aggregate-range-other-input`, `schema-lookup-source-range`, `schema-lookup-source-range-driver`, `schema-lookup-intermediate-range-driver`, `schema-aggregate-range-driver`, `schema-between-endpoints`, `schema-between-adjacent-windows`, `sdtm-lb-lab-specific-ranges`, `adam-advs-window-table`, `adam-adtr-nadir`, `adam-adsl-new-therapy`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_variable_forms.json`, `rust/tests/host-fixtures/cases.json`.

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
([LOOKUP-15](#lookup-15)). An omitted bound leaves that side unbounded;
`closed` still decides whether the stated endpoint is included. Every kind of
lookup may declare it: a named intermediate, a qualified aggregate, and an
implicit join through a structured `source:` binding or a filtered source
([LOOKUP-1](#lookup-1)). The range is
compared only on the records whose keys match, and a record missing a stated
bound is ineligible rather than open-ended. A missing range value seeks no
record; it never widens the match to every record whose keys match.

| `closed` | Lower comparison | Upper comparison |
| --- | --- | --- |
| `both` | `lower <= value` | `value <= upper` |
| `left` | `lower <= value` | `value < upper` |
| `right` | `lower < value` | `value <= upper` |
| `neither` | `lower < value` | `value < upper` |

Only stated bounds are compared. Adjacent windows `[1, 8)` and `[8, 15)`
therefore match day 8 once with `closed: left`. Equal bounds match only their
endpoint under `both`; reversed bounds match nothing. Missing values and
missing stated bounds keep the same no-match behavior for every mode.
This `closed` selects range endpoints. The `closed` of
[`diff`](temporal.md#time-5) counts elapsed boundaries, the `closed` of
[`cut`](expressions.md#expr-13) assigns classification edges, and
[`baseline_flag`](windows.md#window-9) uses its own `inclusive` comparison.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
    between: {value: AGE, lower: AGELO, upper: AGEHI}
```

The value names what the current row holds, and each form has one meaning: a
bare name is an output column ([NAME-3](names.md#name-3)); a name qualified by
the row's driver is a field of the record that built the row; and a name
qualified by a named intermediate's id is the column it selects for the row. A
bare name that no column takes names nothing the row holds, even when the
driver's record stores a field of that name. A value read from a driver's
record must name the driver of every row that reads the lookup, wherever the
lookup is read: in a row template, that template's driver
([LOOKUP-2](#lookup-2)), and during column derivation, the driver of every row
template the read can evaluate on: the templates that leave the column to
column derivation. A row template's `filter` reads its lookups for that
template's rows, and a dataset verification for the rows of every template. A
named intermediate is also read through an aggregate's `derivations` and
through the range value of another lookup. A value naming nothing the current
row holds, such as a bare name that no column takes or a field of another
input or of the lookup's own relation, fails as `unknown_field` at
`between.value`, once per range, whichever kind of lookup declares it and
whatever its bounds bind. Which columns a specification declares and which
input drives each row are declared, so `check` reports a bare name that no
column takes, and a value qualified by an input that does not drive every row
reading the lookup, from the declarations alone, whether or not that input
stores the field. A named intermediate that another intermediate's derivation
reads matches for the donor record there ([LOOKUP-16](#lookup-16)), where its
bare value is that record's field; a row that reads it finds no value in a
bare name that no column takes all the same. A named intermediate binds its
range once for every row that reads it, so its value cannot read the record
that built a row from another named intermediate's records: qualified by the
intermediate that builds a reading row, it is not supported and fails as
`unsupported_operation` at `between.value`
([COLUMN-15](columns.md#column-15)); for rows another input builds, it is a
keyed lookup of a row driver, which declares no key
([LOOKUP-3](#lookup-3)). A field of another input is never read through the
implicit join ([LOOKUP-1](#lookup-1)); bring it onto the row as a column
first, and name the column:

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
bounds, and the named intermediate's id or the source binding's variable that
declares the range; a type decided per value fails the same way when the
record is compared. A derived bound has its derivation's declared or result type
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
Evidence: `schema-lookup-filters`, `negative-source-missing-field`, `negative-source-trivial-filter`, `negative-row-filter-lookup-scope`, `schema-aggregation`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`.

For an input dataset, `order_by` names qualified fields of that dataset only,
`columns` lists bare field names, and every `filter` field is qualified,
except a donor-record name in a donor read ([LOOKUP-16](#lookup-16)). The
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
named intermediates. When an intermediate derivation reads a named
intermediate, the reading donor record supplies the correlated fields,
including its earlier derived fields, bare or qualified by its dataset
([LOOKUP-16](#lookup-16)).

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
stored order, except that `STR_JOIN` joins them in the aggregate's `order_by`
order.

Fails: `unpaired_fields`, `multiple_matches`.
Evidence: `negative-lookup-unordered`, `negative-lookup-empty-order`, `negative-lookup-unchosen`, `negative-source-duplicate-key`, `negative-join-unordered`, `negative-sum-order-by`, `negative-sum-distinct`, `adam-adcm-related-events`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/aggregates.json::fields_beside_a_reduction_that_reads_neither_share_one_finding_in_the_tables_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::an_intermediate_order_over_an_int_and_a_float_keeps_the_record_whose_exact_value_sorts_last`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::an_intermediate_order_over_text_and_an_int_is_refused_before_any_record_is_read`.

A lookup never multiplies rows: of several matching partners, `order_by` with
`keep` reads one, which is a choice, not a join, and the other partners take no
part in the row. One row for each matching pair of records is a row template
driving on a pair relation ([LOOKUP-27](#lookup-27)).

Declaring one of `order_by` and `keep` without the other fails as
`unpaired_fields`, on a named intermediate and a source binding alike, and an
unhandled multiple match fails as `multiple_matches`. An `order_by` that lists
no term orders nothing: `keep` beside it fails as `unpaired_fields` the same
way, reporting `order_by` missing. During grouped row
construction it fails at the join phase, before any column is derived. A
`keep` that chose among several candidates counts one `multiple_matches`
handling ([COLUMN-14](columns.md#column-14)).

The `order_by` terms sort under [VALUE-22](values.md#value-22) and name fields
of the lookup's relation, in the scopes of [LOOKUP-8](#lookup-8). A field the
lookup derives orders as its result type does ([LOOKUP-15](#lookup-15)): one
that holds an `int` or a `float` orders by exact value, and one whose types do
not all compare, such as `str` or `int`, fails validation as
`incompatible_input_type` at the term before any record is read
([VALUE-22](values.md#value-22)). A lookup
whose key is verified unique at `error` severity needs neither `order_by` nor
`keep`, and one whose uniqueness check only warns declares both, or
`nearest` ([LOOKUP-17](#lookup-17)). Every reducer but `STR_JOIN` reads an
aggregate's candidates in stored order ([FORMULA-19](formulas.md#formula-19)).
A named intermediate may select by distance instead, with `nearest`
([LOOKUP-26](#lookup-26)).

An aggregate's `order_by` orders only the candidates `STR_JOIN` joins
([FORMULA-18](formulas.md#formula-18)), and an aggregate declares no `keep`.
Its terms sort under [VALUE-22](values.md#value-22) and name fields of the
aggregate's relation, in the scopes of [LOOKUP-8](#lookup-8), or a variable
its `derivations` bind ([LOOKUP-22](#lookup-22)). Candidates equal on every
term keep stored order ([VALUE-23](values.md#value-23)), so the joined text is
the same on every run. A field of the aggregate and a reducer that reads it
are declared together, as the field table of
[FORMULA-18](formulas.md#formula-18) pairs them: a field beside no reducer
that reads it, and a reducer without a field it needs, fail as
`unpaired_fields` at the aggregate, before any input is read. An aggregate's
`order_by` that lists no term orders nothing either, so `STR_JOIN` beside it
reports `order_by` missing.

<a id="lookup-26"></a>

### LOOKUP-26. A named intermediate's nearest selection chooses the candidate whose date or datetime lies nearest the current row's reference

`nearest` selects one candidate of a named intermediate by distance, in place
of `order_by` and `keep`. A candidate's distance is the absolute difference
between its `source`, a `date` or `datetime` field of the lookup's relation,
and `to`, the current row's reference of the same type, counted in `unit` as
`diff` counts it ([TIME-4](temporal.md#time-4)). The nearest candidate is
chosen, a tie going to the earlier `source` and then to the earlier stored
record; a candidate whose distance is missing is never chosen.

Fails: `prohibited_construct`, `incompatible_input_type`, `unknown_field`.
Evidence: `schema-lookup-nearest-datetime`, `schema-lookup-nearest-date`, `schema-lookup-nearest-missing`, `negative-lookup-nearest-keep`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_nearest_selection_refuses_a_keep_written_beside_it`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_nearest_selection_refuses_an_order_and_a_keep_written_beside_it_naming_the_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_nearest_source_names_a_field_of_the_donor`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_nearest_source_is_a_date_or_a_datetime`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_nearest_reference_has_the_type_of_its_source`.

`nearest` declares `source`, `to` and `unit`, each required. It is the
select step of a named intermediate ([LOOKUP-25](#lookup-25)) and replaces
the ordered selection of [LOOKUP-10](#lookup-10): an `order_by` or a `keep`
written beside it, an `order_by` with no term included, fails validation
before any input is read as `prohibited_construct`, naming each of them and
then `nearest`, in that order, with the intermediate's id as `intermediate`
and the first of them as `construct`. A row driver declares no selection ([ROW-2](rows.md#row-2)), and a
structured `source:` binding has no `nearest`.

`source` names a field of the lookup's relation, written bare: a stored field
or a name the intermediate derives ([LOOKUP-15](#lookup-15)). A name the
relation does not have fails as `unknown_field` at `nearest.source`, naming
it as `identifier`. Its type, declared or its derivation's result type
([EXPR-14](expressions.md#expr-14)), must be `date` or `datetime`; another,
such as that of collected date text, which stays `str`
([INPUT-5](inputs.md#input-5)), fails as `incompatible_input_type` at
`nearest.source`, reporting it as `actual` and `permitted` as `date` and
`datetime`. Complete collected text first with `impute` among the
intermediate's `derivations` ([TIME-8](temporal.md#time-8)), whose rules say
where a partial date lies.

`to` is a value of the current row, read as a key's match value is
([LOOKUP-5](#lookup-5), [LOOKUP-6](#lookup-6)): an output column, a field of
the record of the row's driver, an expression over them, or a literal such as
`DATE '2024-03-10'` or `DATETIME '2024-03-10T08:00'`
([FORMULA-3](formulas.md#formula-3)). It never reads a lookup: derive such a
value into a column first. A field it reads names the driver of every row
that reads the intermediate, or fails as `unknown_field` at `nearest.to`, and
in grouped construction a group key, or fails as `ungrouped_driver_field`
there, as a match value does ([LOOKUP-2](#lookup-2), [LOOKUP-5](#lookup-5)).
It is read with the match values, before any record is read, and a missing
`to` seeks no record, so absence is answered ([LOOKUP-24](#lookup-24)). Its
result type must be the type of `source`; another, a `date` beside a
`datetime` included, fails as `incompatible_input_type` at `nearest.to`,
reporting `expected` and `actual`, before any record is read, or, for a type
decided per value, when the value is read.

`unit` is a unit `diff` counts the source's type in: `day`, `week`, `month`
or `year` for a `date`, and `second`, `minute`, `hour` or `day` for a
`datetime`; another fails as `value_not_permitted` at `nearest.unit`
([TIME-4](temporal.md#time-4)). A distance in a calendar unit counts whole
units, so candidates eight and twelve days from the reference tie at one
week; a distance in an elapsed unit is exact, so every elapsed unit chooses
the same candidate. Collected precision takes no part
([VALUE-21](values.md#value-21)): a value `impute` completed counts as the
day or moment it names. A `date` and a `datetime` never compare
([VALUE-18](values.md#value-18)), so one side is converted explicitly, as
`to: {to_date: {source: EG.EGDTC}}` measures a `date` source against the date
of a `datetime`.

Of the candidates that survive the filter ([LOOKUP-8](#lookup-8)), those with
a distance compete. The smallest distance is chosen; of equal distances, the
earlier `source`, so a candidate before the reference is chosen over one as
far after it; and of equal sources, the earlier stored record. A candidate
whose `source` is missing has no distance, and when no candidate has one,
absence is answered, or `require_match` fails the read as `unmatched_key`
([LOOKUP-24](#lookup-24)). A selection among several candidates with a
distance counts one `multiple_matches` handling at the read's path, as `keep`
does ([LOOKUP-14](#lookup-14)).

```yaml
intermediates:
  - id: NEAREST
    dataset: PC
    key: [STUDYID, USUBJID]
    nearest: {source: PCDTM, to: EG.EGDTC, unit: hour}
    columns: [PCSEQ, PCDTM, PCSTRESN]
    derivations:
      PCDTM: {impute: {type: datetime, source: PCDTC, time: first}}
```

Each electrocardiogram reads the subject's sample nearest to it in time. The
chain of [LOOKUP-15](#lookup-15), a distance derived with `diff` and `ABS`
and an ordered selection over it, can state the same choice with a tie-break
of its own. Every column reading the intermediate shares the selection
([LOOKUP-12](#lookup-12)), and its `unique` checks may warn, a repeated
combination reported and every read still taking the nearest candidate
([LOOKUP-17](#lookup-17)). Another intermediate's derivation that reads it
runs the selection for its donor record, `to` reading that record as a key
value does ([LOOKUP-16](#lookup-16)), and a project function argument or an
aggregate's derivation may read it, as either may read an intermediate that
declares `keep` ([LOOKUP-22](#lookup-22)).

<a id="lookup-24"></a>

### LOOKUP-24. A lookup with no candidate answers absence, missing by default, unless require_match fails it

A lookup with no candidate for a row, or with a missing match or range value,
answers absence: an implicit join gives its `source` binding's `absent` value
and a named intermediate its own `absent` value, either missing when
undeclared and counted under that handler unless a predicate made the read.
`require_match: true` on the binding or intermediate, or on an input for joins
that declare no `absent`, instead fails the read as `unmatched_key`; one
lookup cannot declare both. An aggregate's empty match yields missing
([FORMULA-24](formulas.md#formula-24)).

Fails: `unmatched_key`, `conflicting_absence_policy`, `prohibited_construct`.
Evidence: `schema-lookup-absent-default`, `negative-source-require-match`, `negative-input-require-match`, `negative-lookup-unmatched-key`, `negative-lookup-missing-sex`, `negative-lookup-require-match-no-match`, `schema-handlers`, `adam-adae-death`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-application/tests/fixtures/original_source_bindings.json`.

```yaml
input:
  ADSL: {path: adsl.csv, require_match: true}
  EX: {path: ex.csv, key: [STUDYID, USUBJID]}
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
`absent` value, or missing when `absent` is omitted, independently for each
column read and counted under that column's `absent` handler, declared or by
default. One column's handling never answers for another. `absent: null`
answers missing as an omitted `absent` does; in a child layer it overrides an
inherited answer, and `clear` restores the default
([LAYER-9](inheritance.md#layer-9)).

An aggregate declares no absence handler and no `require_match`. A row whose
match leaves no candidate, or whose match or range value is missing, reduces
an empty group, which yields missing under the empty-group rule of
[FORMULA-24](formulas.md#formula-24); no handler counts it.

A missing match or range value is incomplete, not unmatched: no record is
sought, and the row is answered, or fails, the same way. A matched record
whose value is missing is a collected blank, not an absent record, and is not
answered. A variable that does not exist is no absence and fails under
[NAME-3](names.md#name-3) ([COLUMN-11](columns.md#column-11)), and with
several candidates `order_by` and `keep` choose one first ([LOOKUP-10](#lookup-10)).

`require_match: true` asserts that every row the read is made for finds a
record. On a binding it covers that binding, and on a named intermediate every
read of it. On an input, `input.<NAME>.require_match` covers every read
through an implicit join to that input, the concise form, operands and
predicate reads included, and it may be declared on an input no join reads. A
binding's declared `absent` still answers its own reads, and a named
intermediate over the input keeps its own absence policy: `absent`,
`require_match` or neither. A row that finds no record fails as
`unmatched_key` at the read's path, reporting the sought key values in the
vocabulary of [LOOKUP-14](#lookup-14), and a read that requires a match counts
nothing.

`require_match: true` beside `absent` on one binding or one intermediate, declares both that a row without a record is an error and
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
the type each side declares, whether the lookup or its input declares the
pairs, and the absence policy that answers a miss.

Evidence: `negative-lookup-unmatched-key`, `rust/crates/yamaa-application/tests/fixtures/specs/lookup-failures.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::a_named_intermediate_s_absence_counts_under_the_handler_its_surface_names`.

Neither failure renames the other's fields. Handler accounting follows
[COLUMN-14](columns.md#column-14), whose counts are `handler` rows of the run
report: a `keep` that chose among surviving records, or a `nearest` that
chose among several with a distance ([LOOKUP-26](#lookup-26)), counts one
`multiple_matches` handling, and an absence that a named intermediate answers,
declared or by default, or a record an implicit join does not find counts one
`absent` handling at the read's path ([LOOKUP-24](#lookup-24)). An
`unmatched_key` from an implicit
join names `intermediate(<dataset>)` as its intermediate, as an implicit
join's multiple match does.

## Keys

<a id="lookup-5"></a>

### LOOKUP-5. A key pairs each column of the lookup's relation with a current-row match value

`key` states the match as pairs, each a column of the lookup's relation and
the current-row match value it must equal. A list of column names pairs each
with the same-named current-row value; a mapping pairs each column, written as
its key, with a variable or expression. A named intermediate states its own
`key`; an implicit join, and a qualified aggregate without `key`, use the
input's declared `key`. A lookup names at least one pair, every column must
exist, and every match value must read a known current-row value.

Fails: `invalid_field_type`, `no_applicable_keys`, `unknown_field`, `invalid_aggregate_context`.
Evidence: `negative-lookup-mismatched`, `negative-lookup-unknown-key`, `negative-lookup-repeated-key`, `negative-lookup-key-other-template`, `negative-lookup-key-lookup-qualified`, `negative-aggregate-key-other-template`, `negative-aggregate-key-bare-field`, `negative-verification-lookup-key`, `negative-row-aggregate-keys`, `sdtm-lb-lab-specific-ranges`, `rust/crates/yamaa-application/tests/fixtures/intermediate_key_reference.json`, `rust/crates/yamaa-application/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/aggregate_key_reads.json`, `rust/crates/yamaa-core/src/specification/declared_keys.rs`, `rust/crates/yamaa-core/src/key_relation.rs::a_declared_key_pairs_every_column_it_names`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_variable_forms.json`.

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

An input's `key` is a list: each column it names is paired with the current
row's same-named value, the row's column in column derivation and the row
template's driver field in row construction ([LOOKUP-2](#lookup-2)). An
implicit join to the input, and a qualified aggregate over it that declares no
`key` of its own, match on every column it names. A read of an input that
declares no `key` names no pair and fails as `no_applicable_keys` at the
read, naming the input and the `key` to declare. A named intermediate never
falls back to its input's key ([LOOKUP-3](#lookup-3)).

A written `key` names at least one pair. An empty list or mapping names no
match and fails as `invalid_field_type`, on a named intermediate, an
aggregate and an input alike. A named intermediate's or an input's list names
each column once; a column listed again fails as `invalid_field_type` at that
entry, reporting the column.

A `key` column the lookup's relation does not have fails as `unknown_field`.
Every match value must read a known current-row value: a variable must name
one, and every identifier an expression reads must be one, or the match fails
as `unknown_field`. An input's key is checked where an implicit join or a
keyless qualified aggregate reads it, at that read: a column the input does not
store fails naming `INPUT.COLUMN`, and one the specification declares no column
of fails naming the column. A `key` that no read uses is admitted, and the
lint reports it as `unused_input_key`. `SELF` names donor rows and is never a
current-row scope, so a match value written `SELF.field` names no current-row
value and fails the same way; when the current row carries `field`, the
diagnostic suggests that bare name. A bare name is an output column
([NAME-3](names.md#name-3)), so a field of the driver's record that no column
takes fails too. A match value never reads a lookup: a name qualified by a
named intermediate's id fails as `unknown_field`, whether written alone or
read by an expression ([LOOKUP-11](#lookup-11)). Derive the lookup's value
into a column, then match on the column:

```yaml
columns:
  - {name: SEX, type: str, derivation: SUBJECT.SEX}
intermediates:
  - id: RANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

A written `key` reports its refusals where they are written, for a named
intermediate and a qualified aggregate alike: a column the relation does not
have, and a pair whose types cannot compare, at `key`. A match value that
reads no current-row value fails at a named intermediate's `key` when it is a
name written alone and at `key.<column>` for any other value, and at a
qualified aggregate's pair, `key.<column>`, whatever its form. The pair's
match value is read even when its column is missing. A key an input declares
reports its refusals at the read.

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

Evidence: `adam-adlb-supplb-padded-key`, `adam-adlb-end-of-treatment`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_computed_int_key_of_a_text_donor_column_is_refused_before_matching`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_computed_int_key_of_an_int_donor_column_binds`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/intermediates.json::a_key_whose_type_is_decided_per_value_binds_and_is_compared_when_matched`.

The result type is the expression's ordinary non-missing result: a local
`missing`, `invalid` or `no_match` replacement takes no part, and a non-missing
replacement is compared when it is used as a match operand. A pair fails
before matching only when none of the types the result can take compares with
the key column's type under [VALUE-19](values.md#value-19), and the failure
names that result type. `N + 1` over an `int` `N` is an `int`, a `mapping` to
integer values an `int`, and `greatest` over an `int` and a `float` a number of
either type, which compares with a numeric key column and with no text one.
`null` is always missing and compares with every column. The same
holds for a named intermediate's and an aggregate's `key` alike.

## The implicit join

<a id="lookup-1"></a>

### LOOKUP-1. A dataset-qualified source reads through the implicit join on the input's declared key

A scalar source qualified by an input reads it through the implicit join, a
lookup whose key pairs are that input's declared `key` columns, each matched
against the current row's column of the same name. A structured `source:`
binding adds its `between`, `filter`, `order_by`, `keep`, `absent` and
`require_match` to the join ([LOOKUP-25](#lookup-25)). The join gives one
value per current row and preserves the row. A qualified read of an input
that declares no `key` fails validation, and the qualifier of the row
template's driver is not a join.

Evidence: `schema-lookup`, `schema-source-binding`, `schema-lookup-source-range`, `schema-lookup-source-range-driver`, `sdtm-sv-subject-visits`, `negative-input-require-match`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::an_implicit_join_matches_on_the_input_s_declared_key`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::a_read_of_an_input_without_key_has_no_applicable_keys`.

```yaml
input:
  ADSL: {path: adsl.csv, key: [STUDYID, USUBJID], require_match: true}
  PERIODS: {path: periods.csv, key: [STUDYID, USUBJID]}
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
current row alone ([LOOKUP-9](#lookup-9)). Each declared key column compares
its two sides as the match step does, and [VALUE-2](values.md#value-2) performs
no implicit conversion: a key whose left and right types are not mutually
comparable fails as `incompatible_input_type`, reporting both types
([LOOKUP-9](#lookup-9)). A qualified read of an input that declares no `key`
fails as `no_applicable_keys` at the read, naming the input
([LOOKUP-5](#lookup-5)). An input that drives some row templates and is read on
the rows others build needs its `key` too ([NAME-4](names.md#name-4)). A named
intermediate declaring its own `key` states a match that differs from the
input's.

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

### LOOKUP-2. Row construction reads another dataset through the same join, using only values available before column derivation

During ungrouped row construction, a scalar source qualified with another
input dataset joins that dataset on the input's declared key, each column
matching the same-named field of the row template's input record. During
grouped row construction each key column instead matches the candidate's
group-key value, and every key column must be a group key of the row
template. A match value must be available during row construction; one
available only during column derivation fails.

Fails: `ungrouped_driver_field`, `unknown_field`, `phase_boundary`.
Evidence: `negative-grouped-lookup-ungrouped-key`, `negative-grouped-range-ungrouped-value`, `negative-lookup-key-column-phase`, `negative-lookup-match-column-phase`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs`, `adam-adlb-supplb-padded-key`, `schema-lookup-source-range-driver`, `schema-lookup-source-range-default`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_row_phase_boundaries.json`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::a_row_template_s_join_matches_the_driver_on_the_declared_key`.

In ungrouped row construction each retained input record builds one candidate
row ([ROW-3](rows.md#row-3)). The input record's fields are
the only single values the match can read; values produced only by column
derivation are not available then. A named intermediate states the same match
with declared `key` pairs, and its match values have the same availability:
input-record fields or columns of the same row template. The join binds one
value per row, and later derivations in the same row template read that value
through the bound column. Text in an expression field during row construction
still names no qualified cross-dataset identifier
([FORMULA-9](formulas.md#formula-9)).

In grouped row construction every declared key column must be a group key of
the row template ([ROW-4](rows.md#row-4)). A key the group does not
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
column derivation, and a match value available only then fails as
`phase_boundary`. A named intermediate declares its match once for every read,
so the finding is at the row template's read of the intermediate, in a
derivation or the row `filter`, once for that site. It names the value, the
intermediate and the row template, whether the value is a `key` match value,
the intermediate's `between` value, or a match value of another intermediate
that a `between` value reads; in that last case the finding also names each
value followed, from the first `between` value to the value that reads the
column. A structured source's own `between` value is written in the row
derivation, and one available only during column derivation fails as that
derivation's read does ([COLUMN-4](columns.md#column-4)), at `between.value`.

## Named intermediates

<a id="lookup-3"></a>

### LOOKUP-3. A named intermediate declares an id, a dataset and a key

An entry of `intermediates` declares `id`, `dataset`, the relation it reads,
and `key`, its match ([LOOKUP-5](#lookup-5)), unless it drives a row
template or is a side of a pair relation, which declare none
([ROW-2](rows.md#row-2), [LOOKUP-27](#lookup-27)). Its `id` shares one naming
list with dataset identifiers, other lookup ids and the output `domain`, and a
collision fails. An intermediate read as a lookup without `key` fails as
`missing_required_field`.

Fails: `duplicate_identifier`, `missing_required_field`.
Evidence: `negative-lookup-id-collision`, `negative-lookup-unknown-key`, `schema-lookup`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::an_intermediate_read_as_a_lookup_requires_its_key`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::a_row_driver_read_as_a_lookup_is_refused_at_its_key`, `rust/crates/yamaa-adapters/tests/declaration_v1_lowering.rs::an_intermediate_without_key_is_refused_unless_it_drives_rows`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/one_spelling.json::an_intermediate_read_as_a_lookup_without_key_is_refused_at_its_key`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/one_spelling.json::an_intermediate_nothing_reads_without_key_is_refused_at_its_key`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::an_unread_lookup_without_key_is_refused_at_its_key_and_keeps_no_input_live`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/layer_composition.json::an_unread_lookup_without_key_or_filter_is_refused_at_its_key_and_keeps_no_input_live`, `rust/tests/host-fixtures/cases.json::ordinary_lookup_without_keys`.

For a named intermediate, `dataset: SELF` instead reads the specification's
completed derived rows ([LOOKUP-7](#lookup-7)). The current row is the output
row, or grouped-row candidate, the lookup runs for. The match fields are the
`key` columns, the match values are what the current row supplies for them
([LOOKUP-5](#lookup-5)), and the candidates are the matched records that
survive `filter` ([LOOKUP-25](#lookup-25)).

A `dataset` that names a pair relation reads the relation's records as it
reads an input's, with a `key` of its own ([LOOKUP-27](#lookup-27)). A pair
relation's own `key` pairs its two sides, not a current row, and an
intermediate that is a side of one declares no `key`, as a row driver does
([ROW-2](rows.md#row-2)).

An `id` collision fails as `duplicate_identifier`. The schema requires `id`
and `dataset`: omitting either fails as `missing_required_field` with no
rule cited. An intermediate that drives no row template, is no side of a pair
relation and declares no `key`, or a pair relation without one, fails as
`missing_required_field` at its `key` before any input is read, even when
nothing reads it, its context naming the field and its class as the schema's
does; a row template's driver read as a lookup by the rows another template
builds fails the same way. An intermediate other than a pair relation that
lacks its `key` and that nothing reads survives pruning only to reach this
refusal: it reads nothing, so an input that only it names is pruned
([LAYER-11](inheritance.md#layer-11)) and is not held to the fields it lacks. An intermediate never falls back to its input's
`key`.

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
`require_match: true`. An intermediate that declares nothing beyond `id`,
`dataset` and a `key` listing its input's declared `key`, with or without
`absent: null`, reads exactly what the implicit join reads
([LOOKUP-1](#lookup-1)) and fails: read the input dataset directly instead of
aliasing it. Composition preserves this refusal even when no output reads the
declaration.

Fails: `rename_only_intermediate`.
Evidence: `negative-lookup-rename-intermediate`, `sdtm-cm-whodrug-coding`, `rust/crates/yamaa-core/src/specification/declared_keys.rs::an_intermediate_keyed_as_its_input_only_renames_it`.

```yaml
# rejected: CODED adds nothing to CODING
input:
  CODING: {path: coding.csv, key: [STUDYID, USUBJID, CMSEQ]}
intermediates:
  - id: CODED
    dataset: CODING
    key: [STUDYID, USUBJID, CMSEQ]
    absent: null
```

Without `absent` the intermediate answers a row without a record with
missing as well ([LOOKUP-24](#lookup-24)), so omitting the line changes
nothing. The key's columns compare in any order, as pairs do
([LOOKUP-5](#lookup-5)); a key that names other columns, or is written as a
mapping, states a match of its own. Asserting a match is a change the
implicit join cannot state: `require_match: true` makes the same declaration
valid.

Instead, qualify the dataset where it is read:

```yaml
intermediates:
  - id: DRUG
    dataset: WHODRUG
    key:
      DRUG_RECORD_NO: CODING.DRUG_RECORD_NO
      ATC_CODE: CODING.ATC_CODE
    absent: null
```

An implicit join to `CODING` also needs matching key types. When the
output's `CMSEQ` is numeric but `CODING.CMSEQ` is stored as text, declare
`CODING.CMSEQ` as `int` on input before reading `CODING.DRUG_RECORD_NO`.

<a id="lookup-7"></a>

### LOOKUP-7. A named intermediate may read SELF, the completed rows of earlier row templates

A lookup's `dataset` must be declared in `input` or be a pair relation
([LOOKUP-27](#lookup-27)), except that a named intermediate may use `SELF` when
the specification declares row templates.
`SELF` holds the rows of completed earlier templates during row construction
and all completed rows during column derivation; the current template's
unfinished rows are never eligible, and a read from the first template fails.
`SELF` is reserved for this purpose, cannot also name an input, and is read
only through a named intermediate, never as a directly qualified source.

Fails: `phase_boundary`, `unknown_field`.
Evidence: `adam-adae-prior-serious-event`, `adam-advs-windows`, `negative-self-window-first-template`, `negative-self-filter-first-template`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::completed_templates_are_borrowed_and_republished_in_construction_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::empty_completed_template_uses_literal_absence_in_later_templates`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::later_templates_read_completed_rows_through_between_values`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_later_template_s_mapping_reads_a_completed_self_donor_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_later_template_s_cut_reads_a_completed_self_donor_type`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::first_template_read_preserves_full_phase_context_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::hidden_first_template_alternative_preserves_its_authored_phase_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::hidden_first_template_first_available_source_preserves_its_authored_phase_path`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_aggregate_keeps_the_self_field_it_reads`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_aggregate_keeps_the_self_record_star`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_aggregate_in_a_case_result_keeps_the_self_record_star`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_mapping_of_a_self_field_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_cut_of_a_self_field_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_read_through_a_between_value_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_read_through_two_between_values_names_each_in_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_case_predicate_read_through_a_between_value_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_filter_read_of_self_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_filter_read_through_a_between_value_fails_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_first_template_filter_read_through_two_between_values_names_each_in_order`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::a_grouped_first_template_filter_reads_no_qualified_name`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::column_only_donor_is_unavailable_to_later_templates`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::an_input_named_self_is_refused_before_study_access`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/declarative_self.json::self_intermediates_without_row_templates_are_refused_before_study_access`.

A read from the first template fails as `phase_boundary`. So does a
first-template read of another named intermediate whose `between` value reads
a `SELF` intermediate, directly or through the `between` values of further
intermediates; the finding names the read, then each `between` value in the
order followed. The first template's row `filter` reads for that template's
candidates, so a read there fails the same way, at `rows[0].filter`; a
grouped template's `filter` reads no qualified name at all
([ROW-6](rows.md#row-6)). The donor fields are output columns derived in every
row template; a column derived only later, in column derivation, is
unavailable to `SELF`. The order of donors follows row construction order.

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
deriving `AVISIT: "'WEEK 4'"` reads the same intermediate for its own
window.

```yaml
intermediates:
  - id: COVER
    dataset: SELF
    key: {USUBJID: VS.USUBJID, PARAMCD: VS.VSTESTCD, AVISIT: AVISIT}
    order_by: [SELF.VSSEQ]
    keep: last
rows:
  # ...after the template that builds the collected rows
  - id: expected_week2
    dataset: VS
    group_by: [VS.USUBJID, VS.VSTESTCD]
    filter: "COVERED IS NULL"
    derivations:
      AVISIT: "'WEEK 2'"
      COVERED: COVER.VSSEQ
```

<a id="lookup-11"></a>

### LOOKUP-11. A lookup-qualified variable reads a column of the selected record

A variable qualified by a lookup id, such as `DEATHEV.AEDECOD`, reads the named
column of the selected record in any field typed as `variable` except a key
match value, which never reads a lookup ([LOOKUP-5](#lookup-5)). A declared
`columns` list restricts which columns the lookup may read; without it every
stored and derived column is available. A column the lookup's dataset does not
have, or one outside a declared `columns` list, fails.

Fails: `unknown_field`.
Evidence: `adam-adae-death`, `negative-lookup-key-lookup-qualified`, `rust/crates/yamaa-core/tests/relations_reference_scope.rs`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_variable_forms.json`.

Naming in `columns` a column the dataset does not have fails as
`unknown_field`, and so does a lookup-qualified read of a missing or
unlisted column. In a `between.value` a lookup-qualified variable reads the
column the lookup selects for the row ([LOOKUP-9](#lookup-9)), or in a donor
read for the donor record ([LOOKUP-16](#lookup-16)). In a key match
value it fails as `unknown_field` at the pair, written alone or read by an
expression; derive the value into a column first and match on the column.

<a id="lookup-12"></a>

### LOOKUP-12. A lookup matches once per row when first read and never changes the row count

A named lookup matches only when it is read. The match runs the first time a
column reads the lookup for a row, and every later read in that row shares the
selection, so every column reading the same lookup sees the same record. A
lookup answers one value per current row, the selected record's column, its
absence answer or an `unmatched_key` failure under `require_match`, and never
changes the row count. An input lookup's dataset is read once per run.

Evidence: `rust/crates/yamaa-application/tests/fixtures/diagnostics/suppressed_lookup_reads.json`.

The per-row match selects among records already read and never re-reads the
dataset. An intermediate declaring `require_match: true` never fails as
`unmatched_key` on a row that did not read it. A `SELF` intermediate extends
its donor pool after each completed row template and invalidates its cached
selection index at that boundary.

## Intermediate derivations

<a id="lookup-15"></a>

### LOOKUP-15. Intermediate derivations add per-record fields to the donor records

An intermediate may declare `derivations:`, a map of names to derivations in
the expression language of row-template `derivations:`, evaluated in
declaration order over the dataset's stored fields and the derivations
declared before each one. A typed entry converts each result to its declared
type before anything later reads it, and an untyped entry keeps its raw
result, its field taking the expression's result type
([EXPR-14](expressions.md#expr-14)). The derived values augment donor records
for matching, `filter`, `order_by` selection and `columns` projection, and
behave there like stored fields.

Fails: `unknown_field`, `duplicate_derivation`.
Evidence: `schema-lookup-intermediate-derived-filter`, `schema-lookup-intermediate-correlated-derive`, `schema-lookup-intermediate-closest-datetime`, `schema-window-intermediate-rank`, `negative-lookup-derivation-scope`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::a_lookup_derivation_reading_the_source_record_is_unknown_to_the_donor_template`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_model.json::a_lookup_derivation_read_by_the_source_template_alone_reads_its_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_matched_derivations.json::a_record_no_row_matches_is_never_augmented`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_matched_derivations.json::a_matched_record_that_fails_still_stops_the_run`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_matched_derivations.json::a_derived_key_augments_every_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_matched_derivations.json::a_derived_bound_augments_every_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/lookup_matched_derivations.json::a_window_among_the_derivations_augments_every_record`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::an_intermediate_order_over_a_date_and_a_datetime_is_refused_before_any_record_is_read`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/order_term_types.json::a_window_order_over_text_and_an_int_is_refused_before_any_record_is_read`.

Each entry is either an untyped expression or a typed binding
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
    unconvertible: null
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
intermediate reads only one that declares `keep` or `nearest`
([LOOKUP-22](#lookup-22)); an argument may read the donor record's own fields
as any operand does.

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

An untyped entry's field may take a type decided per value, which a key
compares when it matches ([LOOKUP-9](#lookup-9)). An order term over it, in
the intermediate's `order_by` or a window's order, a `baseline_flag`'s `date`
included, is decided before any record is read: a field whose types do not all
compare, such as `case` choosing `'NONE'` or a number, or `date` or `datetime`,
fails validation as `incompatible_input_type` at the term
([VALUE-22](values.md#value-22)), while one choosing `0` or a `float` orders by
exact value. Give such an entry a `type` to convert its results to one type
first.

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
    key: [STUDYID, USUBJID]
    derivations:
      EOT_FALLBACK: "DSSTDY + DSSEQ / 1000"
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
      DIST: "ABS(DAY - BASE.REF)"
    filter: "SRC.DIST IS NOT NULL"
    order_by: [SRC.DIST]
    keep: first
    columns: [VALUE, DIST]
```

The `filter` drops every `SRC` record whose distance is missing. When the
current input record's reference is missing, every distance is missing, no
candidate survives, and the read answers absence: missing, since `PICK`
declares no `absent` ([LOOKUP-24](#lookup-24)). Between two dates or two
datetimes, a `nearest` selection ([LOOKUP-26](#lookup-26)) states this choice
in one field, with a tie rule of its own; the distance derivations below state
one with any tie rule.

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
      DIST: "ABS(GAP)"
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
      ADTM: {impute: {type: datetime, source: DTC, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: BASE.REFDTM, unit: hour}}
      DIST: "ABS(GAP)"
```

`diff` with `type: datetime` reads two `datetime` values
([TIME-1](temporal.md#time-1)), so the reference must be one: a field its
input declares `datetime`, which must hold complete text
([VALUE-15](values.md#value-15)), or a value `impute` completed. A reference
collected as text that may be partial or have no time, such as an SDTM
`--DTC`, takes its own `impute` in the same derivations, and the difference
reads the completed value:

```yaml
      REFDTM: {impute: {type: datetime, source: BASE.REFDTC, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: REFDTM, unit: hour}}
```

`GAP`, and with it `DIST`, is missing when `ADTM` or the reference is missing,
including a reference whose text is coarser than a day, since without a `day`
rule `impute` supplies only the time ([TIME-8](temporal.md#time-8)). A
candidate without a distance is dropped by the filter, or placed last by
`nulls: last` when the filter is left out. For each candidate's `DTC`:

- A missing `DTC` gives a missing `ADTM` without any handler
  ([TIME-13](temporal.md#time-13)).
- Partial collected text such as `2024-03` is valid
  ([TIME-7](temporal.md#time-7)), so `invalid` never answers it. Without a
  `day` rule it gives a missing `ADTM` and has no distance
  ([TIME-8](temporal.md#time-8)). A declared `day` rule completes it, and a
  `month` rule, which completes a year alone, needs one too
  ([TIME-10](temporal.md#time-10)): with `day: first`, `2024-03` becomes
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
  matches its key, unless `invalid: null` answers it with a missing
  `ADTM` ([COLUMN-10](columns.md#column-10)).

`time: first` is not neutral for a distance. Against a 09:00 reference, a
date-only candidate on the same day is imputed to 00:00, 9 hours away, and a
14:00 candidate, 5 hours away, wins; `time: last` moves the date-only
candidate to 23:59:59 instead. How a missing time, day or month is completed,
for a candidate or the reference, comes from the statistical analysis plan.
When the imputed datetime is published, publish its imputation flags beside
it: derive `ATMF: {imputation_flag: {source: ADTM, part: time}}` after `ADTM`
([TIME-18](temporal.md#time-18)), and a `part: date` flag as well when a
`day` or `month` rule supplies a date field, and list them in `columns`. `ATMF`
is `H` for a date-only candidate, and `seconds_collected: false` keeps a study
that collects times to the minute from flagging every timed candidate `S`.

To exclude the candidates collected without a time instead, read the
precision of each candidate's text ([TIME-7](temporal.md#time-7)) and keep
those collected to the minute or the second:

```yaml
    derivations:
      PREC: {precision: {type: datetime, source: DTC}}
      ADTM: {impute: {type: datetime, source: DTC, time: first}}
      GAP: {diff: {type: datetime, start: ADTM, end: BASE.REFDTM, unit: hour}}
      DIST: "ABS(GAP)"
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
Evidence: `adam-adlb-end-of-treatment`, `adam-adsl-crossover-sequence`, `schema-lookup-donor-range`, `schema-lookup-donor-derived`, `negative-lookup-donor-range-driver`, `rust/crates/yamaa-application/tests/execution_aggregate_record_operations.rs`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/check_variable_forms.json`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/project_functions.json::an_intermediate_callback_reads_a_kept_selection_beside_its_own_donor_field`.

Either form of a donor-record name names a stored field or a derivation
declared before the reading one, and a name the donor record cannot supply
fails as `unknown_field`. The read column must exist in the other
intermediate's dataset or derivations and, when that intermediate declares
`columns`, be one of them ([LOOKUP-11](#lookup-11)).

In the other intermediate's `between.value` the bare form is the donor
record's field even when no output column takes the name, while a row that
reads the same intermediate still finds no value there
([LOOKUP-9](#lookup-9)). A value qualified by a third intermediate's id reads
that intermediate's column for the donor record, and that match too reads the
donor record alone. A read of any other record fails as `unknown_field` at
the reading derivation, naming the read as `identifier`, the reading
intermediate and the intermediate it reads as `read`: a range value,
correlated `filter` field or derivation field of the other intermediate that
names another input's record, such as the record of the current row's
driver, and a key value of an intermediate its range value reads that does
so. A key match value never reads an intermediate ([LOOKUP-5](#lookup-5)).

The other intermediate is selected once per donor record, and every derivation
of that record that reads it shares the selection ([LOOKUP-12](#lookup-12)). A
read that fails, such as an intermediate declaring `require_match: true` that
selects nothing, fails the run with that intermediate's condition at that
intermediate's path, as a row reading it would. The read adds no current-row
dependency and never changes the number of donor records. Any operand may read
an intermediate without `keep`, which fails as `multiple_matches` when it
matches several records for a donor record ([LOOKUP-10](#lookup-10)), except a
project function argument, which may read only an intermediate that declares
`keep` or `nearest` ([LOOKUP-22](#lookup-22)).

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

### LOOKUP-17. An intermediate's unique verification asserts that no donor column combination repeats, warning instead only where a selection chooses

An intermediate may declare `verifications:`, a list of `unique` checks, each
asserting that its non-empty column combination is unique across the
intermediate's source-only filtered donor records with `derivations:`
computed. Input-backed checks run before any row is built, and `SELF` checks
run after each completed template. A repeated combination fails the run,
except under `severity: warning`, which only an intermediate selecting by
`keep` and `order_by`, or `nearest`, may declare: the repeated combinations
are reported and the selection chooses. A lookup whose key is verified unique
at `error` needs no selection.

Fails: `duplicate_intermediate_records`, `unknown_field`, `correlated_filter_with_unique_verification`, `correlated_derivation_with_unique_verification`, `value_not_permitted`.
Evidence: `schema-lookup-intermediate-unique`, `negative-lookup-duplicate-intermediate`, `negative-lookup-inner-join-duplicate-pairs`, `schema-lookup-intermediate-unique-warning`, `negative-lookup-unique-warning-unordered`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/unique_key_equality.json::a_unique_key_holding_an_int_and_an_equal_float_repeats_as_key_matching_meets_them`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/unique_key_equality.json::a_warning_unique_reports_an_int_and_an_equal_float_and_the_selection_chooses_between_them`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/unique_key_equality.json::a_unique_key_counts_two_missing_values_as_one_combination_though_no_row_matches_them`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_warning_unique_without_a_selection_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_warning_unique_beside_keep_alone_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_warning_unique_beside_an_order_of_no_term_is_not_permitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_warning_unique_beside_an_order_and_keep_is_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_warning_unique_beside_nearest_is_admitted`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::an_error_unique_without_a_selection_is_admitted`, `rust/crates/yamaa-application/tests/fixtures/diagnostics/run_report.json::a_warning_unique_beside_a_nearest_selection_reports_and_reads_take_the_nearest`.

```yaml
- id: DS_EOS
  dataset: DS
  key: [STUDYID, USUBJID]
  filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.DSDECOD <> 'SCREEN FAILURE'"
  verifications:
    - unique: [STUDYID, USUBJID]
```

Two records repeat a combination when each of its columns holds values equal
as a key compares them ([LOOKUP-9](#lookup-9)): exactly under
[VALUE-19](values.md#value-19), so an `int` and a `float` of equal value, such
as `1` and `1.0` of an untyped derivation ([LOOKUP-15](#lookup-15)), are one
value. A key whose columns check unique therefore matches at most one record.
Unlike a key, a missing value is a value of the combination: two records
missing it, and equal in the other columns, repeat, although no row matches
either.

A pair relation's checks run over its records, the matching pairs its filter
keeps with their derivations complete, once the relation is built and before
any row ([LOOKUP-27](#lookup-27)), and a repeated combination names the
relation itself as the dataset it repeats in. It selects nothing, so its
checks take `error` severity alone, and each column a check names is a
derivation of it.

The mapping form `unique: {columns: [STUDYID, USUBJID], id: donor-key}` also
admits an optional `severity`, and it is written only to declare one of them;
both forms test the same donor records, and a mapping holding only `columns`
restates the list and fails as `redundant_long_form` under
[SCHEMA-15](reference/schema-language.md#schema-15). Each check may declare an
optional `id`, governed by
[CHECK-3](verification.md#check-3). [LOOKUP-10](#lookup-10) already rejects
multiple surviving matches without a selection rule.

`severity` is `error` or `warning`, defaulting to `error`, as for a column or
dataset verification ([CHECK-2](verification.md#check-2)). On an intermediate
that declares neither `keep` with `order_by` nor `nearest`, `severity: warning`
fails validation before any input is read, as `value_not_permitted` at the check's
`severity`, with context `intermediate` (its id), `value` (`warning`) and
`permitted` (`[error]`). A violated `warning` check fails nothing and changes
no donor record. The run continues as [CHECK-2](verification.md#check-2)
describes, and every read of the intermediate selects one candidate by its
selection ([LOOKUP-10](#lookup-10), [LOOKUP-26](#lookup-26)).

```yaml
- id: LAST_EX
  dataset: EX
  key: [STUDYID, USUBJID]
  order_by: [EX.EXSTDTC, EX.EXSEQ]
  keep: last
  verifications:
    - unique: {columns: [STUDYID, USUBJID], severity: warning}
```

A subject with several exposure records is reported, and its latest record is
read. A `warning` check is a row of the run report
([CHECK-23](verification.md#check-23)), holding a violated check's repeated
combinations ([CHECK-24](verification.md#check-24)), and its specification
must declare `output.report` ([CHECK-22](verification.md#check-22)).

A repeated combination fails an `error` check as
`duplicate_intermediate_records` at the
`intermediates.<id>.verifications[<j>].unique` path, and a violated `warning`
check's report row names the same condition. Each `unique` column must name
a stored field or a derived name, or fail as `unknown_field`. A `filter` or
`derivations:` entry that fails to materialize fails the run here as well. A
`filter` that references the current input record is evaluated per record and
cannot combine with `verifications:`; the combination fails validation as
`correlated_filter_with_unique_verification`. A derivation that reads the
current input record likewise cannot combine with `verifications:` and fails
as `correlated_derivation_with_unique_verification`. `keep` stays a positional
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

Fails: `invalid_aggregate_context`, `mixed_relations`.
Evidence: `schema-aggregation`, `rust/crates/yamaa-application/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-application/tests/fixtures/row_reductions.json`.

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
  aggregate keyword makes the same qualifier relational. The declared
  dataset is an input or a pair relation, whose records it reduces as an
  input's, by a `key` of its own ([LOOKUP-27](#lookup-27)).
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
the results in an expression field's text, which admits a qualified identifier
only for a record selected by a named intermediate and still rejects an
arbitrary dataset-qualified identifier ([FORMULA-9](formulas.md#formula-9)).
Every join remains under these lookup rules.

A qualified name in an aggregate expression reaches a field of its relation,
never an ODM item by its `ItemOID`, and a qualified name that names an ODM item
fails. ODM item identifiers carry further periods; read the item into a column
with [`odm`](names.md#name-7) first.

The context decides what `group_by` ([LOOKUP-19](#lookup-19)), `key`
([LOOKUP-5](#lookup-5)), `between` ([LOOKUP-9](#lookup-9)), `filter`
([LOOKUP-8](#lookup-8)) and `derivations` ([LOOKUP-22](#lookup-22)) range
over; `expr` is the reducer expression of
[FORMULA-17](formulas.md#formula-17).

<a id="lookup-19"></a>

### LOOKUP-19. A qualified aggregate groups by output keys and an unqualified one by output columns

A qualified aggregate's `group_by` declares qualified right-side columns, and
each must also be an output key; the reduction stays coarser than or equal to
the key it joins on. An unqualified aggregate's `group_by`
declares constructed output columns and must declare at least one; a reduction
over the whole output is not registered. A grouped-row aggregate declares no
local `group_by`: the enclosing `row.group_by` already fixes its current
relation and keys.

Fails: `invalid_aggregate_context`.
Evidence: `rust/crates/yamaa-application/tests/fixtures/correlated_aggregates.json`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/compiler.json::a_qualified_aggregate_group_by_column_must_be_an_output_key`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/compiler.json::a_qualified_aggregate_grouped_by_the_output_key_binds`.

A qualified `group_by` column that is not an output key fails, as does an
unqualified expression with no `group_by` or with a `between`. A grouped-row
aggregate fails when it declares its own `group_by`, declares `key`
([LOOKUP-5](#lookup-5)) or `between` ([LOOKUP-9](#lookup-9)), names an
identifier outside its row template's input dataset, or is used by an
ungrouped row template.

<a id="lookup-22"></a>

### LOOKUP-22. A qualified aggregate's derivations compute typed per-record values before reduction

A qualified aggregate may declare `derivations` to bind per-record
intermediate variables before reduction. Each entry, keyed by the variable's
name, declares its `type` and `derivation`; entries evaluate once per record
of the aggregate's relation, in declaration order, and each derivation reads
the record's fields and the values bound by earlier entries. The reducer
expression and the aggregate's `order_by` name each bound variable by its
unqualified name. An entry cannot use a window operation, and `derivations` is
not available on unqualified or grouped-input aggregates.

Fails: `invalid_derive_step`, `unknown_derive_variable`.
Evidence: `schema-derive-text-score`, `schema-derive-date-epoch`, `schema-aggregate-derive-intermediate`, `negative-donor-function-unkept`, `adam-adcm-related-events`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/projects.json::a_donor_function_argument_reads_only_intermediates_that_select_one_record`.

An aggregate derivation is not an expression function. Text becomes a number
only through the entry's declared `type`, and a date becomes an integer only
through an operation such as `to_epoch_day` in the entry's own derivation; an
`aggregate` expression or a derivation's text never parses text or dates
itself. Each entry's evaluated value converts to its declared `type` under
[VALUE-3](values.md#value-3), and an entry whose result type
([EXPR-14](expressions.md#expr-14)) can never convert to it fails validation
under [VALUE-24](values.md#value-24). Text that is not convertible to the
declared numeric type fails as a conversion failure under
[VALUE-3](values.md#value-3), answered by a declared `unconvertible`
handler and fatal otherwise. A missing value stays missing without attempting
conversion.

An `order_by` term that names a bound variable sorts the record by the
variable's converted value ([LOOKUP-10](#lookup-10)). A sequence number stored
as text therefore joins in numeric order once an `int` entry binds it:
`SEQN: {type: int, derivation: RELREC.IDVARVAL}` with `order_by: [SEQN]`
joins `2, 10`, where the text itself sorts `10` before `2`.

The window operations are `row_number`, `rank`, `row_value`, `locf` and
`baseline_flag`, including inside `case`; validation reports
`invalid_derive_step` at the window's path.

An entry's derivation that names an unqualified variable no earlier entry
binds, or a reducer expression that names one no entry binds, fails validation
as `unknown_derive_variable`. The map composes by key under
[LAYER-6](inheritance.md#layer-6), and a finding names an entry
`aggregate.derivations.<name>`.

The relation a derived aggregate reduces is the one relation its derivations
and filter name, and naming two relations is an error. A reducer
expression that mixes a bound variable with a qualified identifier is an error
under [LOOKUP-18](#lookup-18). Qualifiers naming declared named intermediates
do not count as relations for this rule. An entry may read a named
intermediate that declares `keep` or `nearest`: either selects at most one
record per row ([LOOKUP-10](#lookup-10), [LOOKUP-26](#lookup-26)), and the
read is a row-scoped value, not another reduced relation. The entry still
evaluates once per record of the aggregate's one relation, and each record
sees the same intermediate value for its row. An entry that reads a named
intermediate with neither is an error.

A project function called in an intermediate derivation
([LOOKUP-15](#lookup-15)) is invoked once per donor record, so its arguments
follow the same rule: an argument may read a named intermediate that declares
`keep` or `nearest`, and an argument that reads one with neither fails as
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

## The pair relation

<a id="lookup-27"></a>

### LOOKUP-27. A named intermediate that writes inner_join is a pair relation, one record for each matching pair of its two sides

A named intermediate that writes `inner_join` is a pair relation: its
`dataset` is the left side and its `inner_join` the right side, each an input
or an intermediate a row template could drive on ([ROW-2](rows.md#row-2)).
Its records are every left record paired with each right record whose `key`
values equal its own, in left stored order and then right stored order; an
unmatched record is in no pair. It requires `key` and `derivations`, and is
read as an input is. Its select and absence steps are none
([LOOKUP-25](#lookup-25)).

Fails: `prohibited_construct`, `missing_required_field`, `invalid_field_type`, `duplicate_identifier`, `invalid_intermediate_driver`, `unknown_intermediate_driver_type`, `unknown_field`, `incompatible_input_type`.
Evidence: `schema-lookup-inner-join-pairs`, `schema-lookup-inner-join-order`, `adam-adaecm-related-pairs`, `schema-lookup-inner-join-aggregate`, `schema-lookup-inner-join-filter`, `schema-lookup-inner-join-missing-key`, `negative-lookup-inner-join-keep`, `negative-lookup-inner-join-no-key`, `negative-lookup-inner-join-self`, `schema-lookup-inner-join-lookup`, `schema-lookup-inner-join-grouped`, `schema-lookup-inner-join-expression-key`, `schema-lookup-inner-join-unconvertible`, `negative-lookup-inner-join-duplicate-pairs`, `negative-lookup-inner-join-key-type`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_pair_relation_refuses_an_order_and_a_keep_written_beside_inner_join_naming_the_order`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_pair_relation_refuses_self_as_its_right_side`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_pair_relation_pairs_two_relations`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/preflight.json::a_pair_relation_is_no_side_of_another`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/front_end.json::a_pair_relation_without_derivations_is_refused`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_pair_relation_key_pair_compares_its_two_types`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_pair_relation_side_reads_no_other_record`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_pair_relation_exposes_fields_of_one_type`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_read_through_a_pair_relation_s_implicit_join_has_no_applicable_keys`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_keyless_aggregate_over_a_pair_relation_has_no_applicable_keys`, `rust/crates/yamaa-core/tests/fixtures/diagnostics/binding.json::a_pair_relation_side_may_read_a_lookup_keyed_by_its_own_record`, `rust/tests/host-fixtures/cases.json::a_pair_relation_key_beyond_the_key_limit`.

```yaml
intermediates:
  - id: AEMEM
    dataset: RELREC
    filter: "RELREC.RDOMAIN = 'AE' AND RELREC.IDVAR = 'AESEQ'"
    derivations: {SEQ: {type: int, derivation: IDVARVAL}}
  - id: CMMEM
    dataset: RELREC
    filter: "RELREC.RDOMAIN = 'CM' AND RELREC.IDVAR = 'CMSEQ'"
    derivations: {SEQ: {type: int, derivation: IDVARVAL}}
  - id: PAIRS
    dataset: AEMEM
    inner_join: CMMEM
    key: [USUBJID, RELID]
    derivations:
      USUBJID: {type: str, derivation: AEMEM.USUBJID}
      RELID: {type: str, derivation: AEMEM.RELID}
      AESEQ: {type: int, derivation: AEMEM.SEQ}
      CMSEQ: {type: int, derivation: CMMEM.SEQ}
rows:
  - id: pairs
    dataset: PAIRS
    derivations: {USUBJID: PAIRS.USUBJID, AESEQ: PAIRS.AESEQ, CMSEQ: PAIRS.CMSEQ, RELID: PAIRS.RELID}
```

Each adverse event member of a relationship pairs with each medication member
of the same subject's same relationship, and each pair builds one row. A
relationship of two events and two medications gives four pairs.

A side that names an input reads every record of it. A side that names an
intermediate reads the records a row template driving on it would read: those
its filter keeps, its derivations complete, before any pair is formed. Such an
intermediate reads an input and declares none of `key`, `between`,
`order_by`, `keep`, `nearest`, `absent`, `require_match` and `inner_join`, or
the pair relation fails as `invalid_intermediate_driver` at its field that
names the side, naming the side as `intermediate` and each such field, the
side's `dataset` among them when it reads no input. So a pair relation is no
side of another, directly or through an intermediate over it. A side
intermediate whose filter or derivations read another input's record fails
the same way, naming `filter` or `derivations`, and one that exposes a field
without one column type ([EXPR-14](expressions.md#expr-14)) fails as
`unknown_intermediate_driver_type` there. The two sides are two relations: a
side named twice fails as `duplicate_identifier` at `inner_join` and then
`dataset`. A side's `columns` limits the fields the pair relation reads of it.

`key` pairs the two sides, the left record standing where a lookup's current
row stands ([LOOKUP-5](#lookup-5), [LOOKUP-6](#lookup-6)). Its list form pairs
each field it names of the right side with the left side's field of the same
name. Its mapping form pairs each right side field, written as its key, with a
value of the left record: the left side's field, qualified by the left side's
name, or an expression over the left side's fields, which each left record
evaluates before any pair is formed. Each pair compares under key equality
([LOOKUP-9](#lookup-9)), with no operand converted: a missing value matches
nothing, so a record missing a key value, on either side, is in no pair, and a
pair whose types cannot compare fails as `incompatible_input_type` before any
input is read, at `key`, or at `key.<field>` for an expression; a type decided
per value fails the same way when that value is compared. A field a side does
not have, and a name written alone that reads no field of the left side, fail
as `unknown_field` at `key`. A missing `key` fails as
`missing_required_field` at `key`, and one that names no pair as
`invalid_field_type` ([LOOKUP-5](#lookup-5)), and one of more than 64 pairs
is refused as every key is, under [LIMIT-2](limits.md#limit-2). Key values
constant on both sides pair every record of one side with every record of the
other: the relation holds at most as many records as the product of its
sides'.

`filter` narrows the matching pairs ([LOOKUP-8](#lookup-8)). It reads each
side's fields qualified by the side's name, and it is asked of every matching
pair before any derivation is evaluated, so a derivation never fails on a pair
it removes. `derivations` names the fields the relation exposes, in
declaration order ([LOOKUP-15](#lookup-15)): each reads the sides' fields by
their qualifiers and the derivations declared before it by name, and a typed
entry converts its result. Each exposed field has one column type, or the
relation fails as `unknown_intermediate_driver_type` at `derivations`.
`columns` limits the exposed fields to the derivations it lists. A name the
filter, a derivation or `columns` reads that is none of these fails as
`unknown_field`, and the sides' own fields are read nowhere else. A missing
`derivations` fails as `missing_required_field`, and an empty one as
`invalid_field_type`. Its `unique` checks run over its records, at `error`
severity only ([LOOKUP-17](#lookup-17)).

A pair relation selects nothing, ranges over nothing and answers no absence:
`between`, `order_by`, `keep`, `nearest`, `absent` and `require_match`
written beside `inner_join`, and `SELF` as either side, fail before any input
is read as `prohibited_construct`, naming each of them and then `inner_join`,
in the order listed here, with the relation's id as `intermediate` and the first
of them as `construct`: a field's name, or `SELF`.

The relation is built once, before any row is built and before any
intermediate's `unique` check runs, and is read as an input is, through its
id. A row template drives on it, each record building one candidate row
([ROW-2](rows.md#row-2)); a named intermediate whose `dataset` names it looks
it up with a `key` of its own ([LOOKUP-3](#lookup-3)); and a qualified
aggregate with a `key` of its own reduces it ([LOOKUP-18](#lookup-18)), so
`COUNT(PAIRS.*)` keyed by an event counts the event's pairs. It declares no
input `key`, so a read through its implicit join, and a qualified aggregate
over it without `key`, fail as `no_applicable_keys` ([LOOKUP-5](#lookup-5)),
whose hint names the routes that read it: a named intermediate or an
aggregate with a `key` of its own.
