# Lookup and joins

A derivation reads another relation through the implicit join of a
dataset-qualified source, through a named intermediate declared in
`intermediates`, or through an aggregate. Each matches declared or applicable
keys, narrows the records, selects at most one record or reduces them, and
answers absence.

## The implicit join

<a id="lookup-1"></a>

### LOOKUP-1. A dataset-qualified source reads through the implicit join on the applicable output keys

A scalar source qualified by a dataset reads it through the implicit join: one
value per current row, matching the current row's values of the applicable
keys against the dataset's same-named columns. The applicable keys are the
output `keys`, in output-key order, that the dataset also carries, and at
least one is required. The match preserves the left row, and a row with no
match yields missing. The qualifier of the row template's driver is not a
join.

Fails: `incompatible_input_type`, `no_applicable_keys`.
Evidence: `schema-lookup`, `schema-source-binding`, `sdtm-sv-subject-visits`.
Replaces: REQ-0111, REQ-0150, REQ-0151, REQ-0152, REQ-1051, REQ-1052.

```yaml
derivation:
  source: ADSL.TRTSDTM
```

The driver's qualifier reads the input or intermediate record that built the
row. An inferred key must compare equal on both sides, and
[VALUE-2](values.md#value-2) performs no implicit conversion: an
applicable key whose left and right types are not mutually comparable fails as
`incompatible_input_type`, reporting both types. With no applicable key the
intended match is unclear, and the read fails as `no_applicable_keys`. The
author then states the match with a named intermediate declaring its `key`
([LOOKUP-5](#lookup-5)), the same explicit form that serves whenever the
intended keys differ from the applicable keys.

A structured `source:` binding, or a filtered source that carries a variable
with its filter, selection or absent value, keeps its `filter`, `order_by` and
`keep` on the implicit join: the filter narrows the eligible records and the
ordered selection chooses among the survivors exactly as an explicit lookup
would ([LOOKUP-8](#lookup-8), [LOOKUP-10](#lookup-10)).

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
Evidence: `rust/crates/yamaa-core/tests/relations_reference_scope.rs`, `adam-adlb-supplb-padded-key`.
Replaces: REQ-0126, REQ-0156, REQ-0157.

In ungrouped row construction each retained input record builds one candidate
row ([ROW-3](rows.md#row-3)). The input record's fields are
the only single values the match can read; values produced only by column
derivation are not available then. A named intermediate states the same match
with declared `key` pairs, and its match values have the same availability:
input-record fields or columns of the same row template. The join binds one
value per row, and later derivations in the same row template read that value
through the bound column. A row-phase `compute` still names no qualified
cross-dataset identifier ([FORMULA-11](formulas.md#formula-11)).

In grouped row construction every applicable key must be a group key of the
row template ([ROW-4](rows.md#row-4)). A key the group does not
carry varies within the group and names no single match value, and fails as
`ungrouped_driver_field` ([ROW-5](rows.md#row-5)). A key no
input record carries fails as `unknown_field`
([NAME-3](names.md#name-3)), in either mode.

During grouped row construction, every variable a lookup matches on must be
derived by the row template that reads the lookup. The exceptions are the
template's group keys and, for an ungrouped template, the driver record's own
fields. [Column derivation](columns.md) orders row derivations
before column derivation, and a match value available only in a later phase
fails as `phase_boundary`.

## Named intermediates

<a id="lookup-3"></a>

### LOOKUP-3. A named intermediate declares an id and a dataset, and an omitted key is the applicable keys

An entry of `intermediates` declares `id` and `dataset`, the relation it
reads; `key` is optional. Its `id` shares one naming list with dataset
identifiers, other lookup ids and the output `domain`, and a collision fails.
An omitted `key` is the applicable keys of [LOOKUP-1](#lookup-1), as it is for
a qualified aggregate, and with no applicable key the read fails. A `key` is
stated only when the intended match differs from that inferred match.

Fails: `duplicate_identifier`, `missing_required_field`, `no_applicable_keys`.
Evidence: `negative-lookup-id-collision`, `negative-lookup-unknown-key`, `schema-lookup`.
Replaces: REQ-0112, REQ-0113, REQ-0114, REQ-0153, REQ-1048, REQ-1050.

For a named intermediate, `dataset: SELF` instead reads the specification's
completed derived rows ([LOOKUP-7](#lookup-7)). The current row is the output
row, or grouped-row candidate, the match runs for. The match fields are the
`key` columns, the match values are what the current row supplies for them
([LOOKUP-5](#lookup-5)), and the eligible records are the dataset records
surviving `filter`.

An `id` collision fails as `duplicate_identifier`. The schema requires `id`
and `dataset`: omitting either fails as `missing_required_field` with no
requirement attached. An omitted `key` with no applicable key fails as
`no_applicable_keys`.

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

A named intermediate must narrow, derive or reshape its dataset, or require
that every read selects a record. An intermediate that declares nothing beyond
`id`, `dataset` and `no_match: null` reads exactly what the implicit join reads
([LOOKUP-1](#lookup-1)) and fails: read the input dataset directly instead of
aliasing it. Composition preserves this refusal even when no output reads the
declaration.

Fails: `rename_only_intermediate`.
Evidence: `negative-lookup-rename-intermediate`, `sdtm-cm-whodrug-coding`.
Replaces: REQ-1248.

```yaml
# rejected: CODED adds nothing to CODING
intermediates:
  - id: CODED
    dataset: CODING
    no_match: null
```

Instead, qualify the dataset where it is read:

```yaml
intermediates:
  - id: DRUG
    dataset: WHODRUG
    key:
      DRUG_RECORD_NO: CODING.DRUG_RECORD_NO
      ATC_CODE: CODING.ATC_CODE
    no_match: null
```

An implicit join to `CODING` also needs matching output-key types. When the
output's `CMSEQ` is numeric but `CODING.CMSEQ` is stored as text, declare
`CODING.CMSEQ` as `int` on input before reading `CODING.DRUG_RECORD_NO`.

<a id="lookup-5"></a>

### LOOKUP-5. A key pairs each lookup column with a comparable current-row match value

`key` states the match as pairs, each a column of the lookup's dataset and the
current-row match value it must equal. A column name, or a list of column
names, pairs each column with the same-named current-row value; a mapping pairs
each column, written as the mapping key, with a variable or expression written
beside it. Pair order does not change the result. Every column must exist,
every match value must read a known current-row value, and each pair must be
mutually comparable, with no operand converted.

Fails: `invalid_field_type`, `missing_aggregate_keys`, `unknown_field`, `incompatible_input_type`.
Evidence: `negative-lookup-mismatched`, `sdtm-lb-lab-specific-ranges`, `rust/crates/yamaa-adapters/tests/fixtures/intermediate_key_reference.json`.
Replaces: REQ-0115, REQ-0116, REQ-0117, REQ-0118, REQ-0305.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: {TESTCD: LBTESTCD, SEX: SEX}
```

Here the current-row `LBTESTCD` matches the limit table's `TESTCD` column; the
list `[TESTCD, SEX]` would have matched `TESTCD` against a current-row
`TESTCD` that does not exist. A mapping may pair a column with the same-named
value, and a match value may be an expression ([LOOKUP-6](#lookup-6)).

A written `key` names at least one pair. An empty list or mapping names no
match: a lookup fails as `invalid_field_type`, and an aggregate fails as
`missing_aggregate_keys` ([LOOKUP-21](#lookup-21)).

A `key` column the lookup's dataset does not have fails as `unknown_field`.
Every match value must read a known current-row value: a variable must name
one, and every identifier an expression reads must be one, or the match fails
as `unknown_field`. `SELF` names donor rows and is never a current-row scope,
so a match value written `SELF.field` names no current-row value and fails the
same way; when the current row carries `field`, the diagnostic suggests that
bare name.

Each pair must be mutually comparable under
[VALUE-19](values.md#value-19): an intermediate's `key` requires each
match value and its paired key column to have the same comparable type. The
match converts no operand. A pair that cannot compare fails as
`incompatible_input_type` under [VALUE-2](values.md#value-2),
reporting both declared types. [INPUT-5](inputs.md#input-5) gives
an undeclared field of a typeless container the type `str`; a key typed on one
side and defaulted on the other needs repair, not a wider comparison.

<a id="lookup-6"></a>

### LOOKUP-6. A key match value may be an expression whose static result type is checked before matching

A match value in a `key` mapping may be an expression instead of a variable.
It is evaluated against the current row, its value is the match operand for
that position, and a missing result matches nothing, as a missing variable
does. The pair's comparison ([LOOKUP-5](#lookup-5)) uses the expression's
static result type, read from the table below: the operation's ordinary
non-missing result, before any local `missing`, `invalid` or `no_match`
replacement. An operation the table omits is judged at run time.

Evidence: `adam-adlb-supplb-padded-key`, `adam-adlb-end-of-treatment`.
Replaces: REQ-1259.

The table is the complete set of operations with a static type for this
comparison. The operation rules define those results; the table fixes which of
them are checked before matching.

| Static comparison type | Operations |
| --- | --- |
| Referenced variable's type | `source` |
| Declared YAML scalar's type (`str`, `int`, `float`, or `bool`); a missing literal has no static type | `literal` |
| `date` | `date_impute`, `to_date` |
| `datetime` | `datetime_impute` |
| `int` | `date_diff`, `rank`, `row_number`, `study_day`, `to_epoch_day` |
| `float` | `round_half_away_from_zero` |
| `str` | `baseline_flag`, `cut`, `date_precision`, `datetime_precision`, `str_case`, `str_extract`, `str_pad`, `str_replace`, `str_template` |
| Numeric (`int` or `float`) | `compute` |

For `compute`, either possible result type compares with `int` and `float`
under [VALUE-19](values.md#value-19); `float` represents that numeric
comparison class in a validation diagnostic, without changing the computed
value. Every other expression operation states no static type for this check,
even when a particular invocation's inputs could reveal one. For example,
`mapping`, `case`, `greatest` and `least` select values whose types depend on
their inputs, and such pairs are judged by whether their match values compare
at run time. A non-missing local replacement is likewise subject to run-time
comparability when it is used as a match operand.

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
Evidence: `adam-adae-prior-serious-event`, `adam-advs-windows`, `rust/crates/yamaa-adapters/tests/execution_declarative_self.rs`.
Replaces: REQ-0120.

A read from the first template fails as `phase_boundary`. The donor fields are
output columns derived in every row template; a column derived only in the
later column phase is unavailable to `SELF`. The order of donors follows row
construction order.

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
    no_match: null
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

<a id="lookup-8"></a>

### LOOKUP-8. A lookup filter reads donor records and may correlate with the current row's driver

A lookup `filter` identifier must name an available field in an admitted scope,
or fail. The lookup's own qualifier reads a candidate donor record, and a
different qualifier may read the current row's driver. A donor-only filter
selects eligible records once per run; a filter referencing the current driver
is evaluated for each current row against the equality-matched donor records,
before range narrowing and ordered selection, and keeps a candidate only when
the predicate is `TRUE`. A filter on a construct with no records to select
among fails.

Fails: `unknown_field`, `prohibited_construct`.
Evidence: `schema-lookup-filters`, `negative-source-missing-field`, `negative-source-trivial-filter`.
Replaces: REQ-0132, REQ-0133, REQ-0148.

For an input dataset, `order_by` names qualified fields of that dataset only,
`columns` lists bare field names, and every `filter` field is qualified. The
current row's driver is root `base` (or the sole input) without explicit rows,
or the enclosing `rows[].dataset` during row construction. During column
derivation the qualifier must be the driver of every row template that the
expression can evaluate on. The row driver may be an eligible named
intermediate under [ROW-2](rows.md#row-2). Other datasets,
intermediates that are not the current driver, and unqualified output names
are not filter scopes. A filtered scalar source remains donor-only; correlated
predicates are supported by named intermediates.

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

The comparison restricts eligible donors, and ordered selection chooses one.
`PRIOR.AVAL`, `PRIOR.ADT` and other reads share the chosen record, including
its missing fields. The lookup generates no rows.

`FALSE` and `UNKNOWN` exclude a candidate. The donor dataset and equality
index may be cached, but a correlated eligibility result is never reused for
another row. Normal predicate typing and missing-value rules apply without
coercion. A `filter` on a construct with no records to select among (an output
column, a chosen lookup record, a group key) fails as `prohibited_construct`.

## Matching and selection

<a id="lookup-9"></a>

### LOOKUP-9. Eligible records match every key pair by equality and then narrow to the between range

Records surviving `filter` match by equality on every source/key pair, then
narrow by `between`: a record is kept when `lower <= value` and
`value <= upper`. A `between` names one current-row `value` and at least one
of `lower` and `upper`, each a field of the donor record; an omitted bound
leaves that side unbounded. Both endpoints are inclusive, and a record missing
a stated bound is ineligible rather than open-ended.

Fails: `unknown_field`, `incomparable_range_types`.
Evidence: `negative-lookup-bad-range`, `sdtm-lb-lab-specific-ranges`, `adam-advs-window-table`.
Replaces: REQ-0121, REQ-0128, REQ-0134, REQ-1049.

Bounds are bare donor-record field identifiers: stored fields or names the
intermediate derives ([LOOKUP-15](#lookup-15)). A `between` with neither bound
fails validation. A bound naming a field the augmented donor record does not
have, or a `value` that is not a known variable, fails as `unknown_field`.

The value and every stated bound must be mutually comparable: `int` and
`float` compare exactly under [VALUE-19](values.md#value-19), and every
other runtime type must match exactly, with no operand converted. A mismatch
fails as `incomparable_range_types` before any record is compared, reporting
the runtime types of the value and the stated bounds.

<a id="lookup-10"></a>

### LOOKUP-10. Several surviving records need order_by and keep, which choose the first or last

`order_by` and `keep` are declared together or not at all. With them, the
ordered first or last surviving record is chosen, and remaining ties break by
record order. More than one surviving record with no `order_by` and `keep` to
choose by is an unhandled multiple match and fails. During grouped row
construction it fails at the join phase, before any column is derived.

Fails: `unpaired_fields`, `multiple_matches`.
Evidence: `negative-lookup-unordered`, `negative-lookup-unchosen`, `negative-source-duplicate-key`.
Replaces: REQ-0119, REQ-0127, REQ-0135, REQ-0145.

Declaring one of `order_by` and `keep` without the other fails as
`unpaired_fields`, and an unhandled multiple match fails as
`multiple_matches`.

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
lookup answers one value per current row, the selected record's column, the
`no_match` literal or an `unmatched_key` failure, and never changes the row
count. An input lookup's dataset is read once per run.

Evidence: `rust/crates/yamaa-adapters/tests/execution_key_lookup_reads.rs`, `rust/crates/yamaa-adapters/tests/execution_per_template_defaults.rs`.
Replaces: REQ-0136, REQ-0138, REQ-0146, REQ-1264.

The per-row match selects among records already read and never re-reads the
dataset. An intermediate without `no_match` never fails as `unmatched_key` on
a row that did not read it. A `SELF` intermediate extends its donor pool after
each completed row template and invalidates its cached selection index at that
boundary.

<a id="lookup-13"></a>

### LOOKUP-13. An intermediate that selects nothing answers with no_match or fails as unmatched_key

An intermediate that yields nothing is answered by its `no_match` handler, its
absence policy: every column reading it receives the literal, independently
for each column read. Without `no_match` the read fails. A missing match value
is incomplete, not unmatched: it yields nothing before any record is sought,
and the absence policy answers the same way. A selected record whose value is
missing is a collected blank, not an absent record, and the absence policy
does not answer it.

Fails: `unmatched_key`.
Evidence: `negative-lookup-unmatched-key`, `negative-lookup-missing-sex`, `adam-adae-death`.
Replaces: REQ-0124, REQ-0129, REQ-0130, REQ-0131, REQ-0147.

The failure is `unmatched_key`, reporting the source values and the key
columns it sought. One column's handling never answers for another.
`no_match: null` answers with missing when it first declares the field, and it
keeps that meaning when an empty child inherits the specification. A later
layer's null over an inherited `no_match` clears the handler under
[LAYER-9](inheritance.md#layer-9). The answer is recorded under the `no_match`
handler of [Column derivation](columns.md).

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
[Column derivation](columns.md): a `keep` that chose among surviving
records counts one `multiple_matches` handling, and a declared `no_match` that
answered an absence counts one `no_match` handling.

## Intermediate derivations

<a id="lookup-15"></a>

### LOOKUP-15. Intermediate derivations add per-record fields to the donor records before matching

An intermediate may declare `derivations:`, a map of names to derivations in
the expression language of row-template `derivations:`, evaluated in
declaration order over the dataset's stored fields and the derivations
declared before each one. A typed entry converts each result to its declared
type before anything later reads it, and an untyped entry keeps its raw
result. The derived values augment each donor record before `filter`,
matching, `order_by` selection and `columns` projection, and behave there like
stored fields.

Fails: `unknown_field`, `duplicate_derivation`.
Evidence: `schema-lookup-intermediate-derived-filter`, `schema-lookup-intermediate-correlated-derive`, `schema-window-intermediate-rank`.
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
    unconvertible: null
```

`WEEK 02` thus yields the integer `2` before numeric matching. `check` reports
incompatible key types under [VALUE-2](values.md#value-2) when the
declared source types and derivation result types establish the mismatch
without reading data.

A bare name reads the dataset's stored field or an earlier derived name. A
qualified name may read the intermediate's dataset, the current input record
under the same scope as a correlated `filter` ([LOOKUP-8](#lookup-8)), or
another named intermediate under [LOOKUP-16](#lookup-16). A reference to an
unrelated input dataset, a derivation declared later in the same map, or an
unknown field fails as `unknown_field`; a derived name that shadows a stored
column fails as `duplicate_derivation`.

Derivations that read only donor records are computed once per record. When
any derivation reads the current input record, the map evaluates for each
lookup's current record, and its augmented donor records, filters and match
indexes are not reused for another current record. Every column reading the
intermediate still shares the same selection ([LOOKUP-12](#lookup-12)).

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
    no_match: null
```

<a id="lookup-16"></a>

### LOOKUP-16. An intermediate derivation may read another intermediate's selection for its donor record

An intermediate derivation may read a column of another named intermediate
through that intermediate's id. The read runs the other intermediate's match
for the donor record being augmented, not for an output row: every match
value, `between` value and correlated `filter` field the match reads comes from
that donor record, written bare or qualified by the reading intermediate's
dataset. Filtering, matching, range narrowing, ordered selection and the
absence policy apply as for any other read, once per donor record.

Fails: `unknown_field`, `phase_boundary`, `dependency_cycle`.
Evidence: `adam-adlb-end-of-treatment`, `adam-adsl-crossover-sequence`, `rust/crates/yamaa-adapters/tests/execution_aggregate_record_operations.rs`.
Replaces: REQ-1263.

Either form of a donor-record name names a stored field or a derivation
declared before the reading one, and a name the donor record cannot supply
fails as `unknown_field`. The read column must exist in the other
intermediate's dataset or derivations and, when that intermediate declares
`columns`, be one of them ([LOOKUP-11](#lookup-11)).

The other intermediate is selected once per donor record, and every derivation
of that record that reads it shares the selection ([LOOKUP-12](#lookup-12)). A
read that fails, such as an intermediate without `no_match` that selects
nothing, fails the run with that intermediate's condition at that
intermediate's path, as a row reading it would. The read adds no current-row
dependency and never changes the number of donor records.

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
records. Each check may declare an optional `id`, governed by
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

- **Qualified.** During column derivation the expression reduces the right
  side before the join of [LOOKUP-21](#lookup-21), even when the expression
  qualifier equals the current row template's input dataset. A scalar source
  qualified to that input dataset reads one record; the aggregate keyword makes
  the same qualifier relational.
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
dataset-qualified identifier ([FORMULA-11](formulas.md#formula-11)). Every join
remains under these lookup rules.

A qualified name in the aggregate grammar reaches a field of its relation,
never an ODM item by its `ItemOID`, and a qualified name that names an ODM item
fails. ODM item identifiers carry further periods; read the item into a column
with [`odm`](names.md#name-7) first.

The context decides what `group_by` ([LOOKUP-19](#lookup-19)), `filter`
([LOOKUP-20](#lookup-20)), `key` ([LOOKUP-21](#lookup-21)), `derive`
([LOOKUP-22](#lookup-22)) and `between` ([LOOKUP-23](#lookup-23)) range over;
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
unqualified expression with no `group_by`. A grouped-row aggregate fails when
it declares its own `group_by`, declares `key` ([LOOKUP-21](#lookup-21)),
names an identifier outside its row template's input dataset, or is used by an
ungrouped row template.

<a id="lookup-20"></a>

### LOOKUP-20. An aggregate filter selects records from the aggregate's own evaluation context

An aggregate `filter` selects records from its evaluation context: right-side
records for a qualified column derivation, constructed output rows for an
unqualified reduction, and the current input group's records for a grouped row
derivation. [FORMULA-4](formulas.md#formula-4) defines predicate evaluation,
and filtering preserves the order of the retained records
([FORMULA-19](formulas.md#formula-19)).

Evidence: `schema-aggregation`, `rust/crates/yamaa-adapters/tests/fixtures/row_reductions.json`.
Replaces: REQ-0471.

<a id="lookup-21"></a>

### LOOKUP-21. A qualified aggregate matches its relation on key pairs, and a grouped-row aggregate declares none

An aggregate whose expression reads a qualified dataset relation matches on
`key` pairs ([LOOKUP-5](#lookup-5)). An omitted `key` is the applicable keys
([LOOKUP-3](#lookup-3)), and with no pairs at all the aggregate names no match
and fails. Declared `key` columns must exist in the relation and their match
values must be known. A grouped-row aggregate reads its own input group and
declares no key pairs: the group is the match.

Fails: `missing_aggregate_keys`, `unknown_field`.
Evidence: `negative-row-aggregate-keys`, `rust/crates/yamaa-adapters/tests/fixtures/correlated_aggregates.json`.
Replaces: REQ-0140, REQ-0141, REQ-0142.

With no pairs the aggregate fails as `missing_aggregate_keys`. A declared `key`
column the relation does not have, or a match value that is not known, fails
as `unknown_field`, and a pair that cannot compare fails under
[VALUE-2](values.md#value-2).

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
Evidence: `schema-derive-text-score`, `schema-derive-date-epoch`, `schema-aggregate-derive-intermediate`.
Replaces: REQ-1189, REQ-1190, REQ-1191, REQ-1192, REQ-1242.

A derive binding is not an expression function. Text becomes a number only
through the binding's declared `type`, and a date becomes an integer only
through an operation such as `to_epoch_day` in the binding's own derivation;
the aggregate and numeric grammars never parse text or dates themselves. Each
binding's evaluated value converts to its declared `type` under
[VALUE-3](values.md#value-3). Text that is not convertible to the
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

For ordered pairwise reductions, first construct one row per sample and derive
its neighbours with [window operations](windows.md), then derive each segment
as an ordinary column. A second specification reduces the stored segments at
the subject grain. Its input may declare
`SEGS: {path: segments.csv, schema: segments.yaml}` under
[INPUT-10](inputs.md#input-10); the producing specification
completes before the consumer reads its artifact under
[INPUT-10](inputs.md#input-10). For the linear trapezoid, the
producer's segment expression is `0.5 * (PREVC + AVAL) * (ARELTM - PREVT)`,
and the consumer's reduction is `SUM(SEGS.SEG)`. A first sample's missing
neighbour produces a missing segment, which `SUM` skips under
[FORMULA-19](formulas.md#formula-19).

<a id="lookup-23"></a>

### LOOKUP-23. A qualified aggregate's between keeps right-side records whose bounds contain a current-row value

A qualified aggregate may declare `between` to narrow right-side records for
each current row. `value` is a variable the current row can read; `lower` and
`upper` are bare donor-record fields of the aggregate expression's one
right-side relation, at least one is required, and every declared comparison is
inclusive. A missing current-row value admits no right-side record, and a
right-side record with a missing stated bound is ineligible. `between` is
invalid on an unqualified or grouped-row aggregate.

Evidence: `adam-adtr-nadir`, `adam-adsl-new-therapy`, `rust/crates/yamaa-adapters/tests/execution_aggregate_record_operations.rs`.
Replaces: REQ-0330, REQ-0472, REQ-0473, REQ-0474, REQ-0509.

The comparisons are `lower <= value` and `value <= upper`. Omitting one bound
makes the match one-sided without excluding the stated endpoint. The value and
every stated bound must be mutually comparable under
[VALUE-19](values.md#value-19). When no record is admitted the aggregate
result is missing under the empty-group rule of
[FORMULA-24](formulas.md#formula-24); a missing cutoff never reduces the
unrestricted right side.

A `between` on an unqualified or grouped-row aggregate fails, as does one
declaring neither bound, naming a bound outside the qualified relation, or
using incomparable operands.
