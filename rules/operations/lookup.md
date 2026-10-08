---
id: operations/lookup
title: Lookup and joins
status: normative
---

# Lookup and joins

## Requirements

### Declaration

<a id="req-0111"></a>

**REQ-0111.** A dataset-qualified scalar source reads that dataset through
the implicit join: one value per current row, matched on the applicable
keys ([REQ-0150](lookup.md#req-0150)), answering absence as a missing
result. The qualifier for this row template's driver is not a join. That
qualifier reads the input or intermediate record that built the row.

```yaml
derivation:
  source: ADSL.TRTSDTM
```

A structured `source:` keeps its `filter`, `order_by`, and `keep` on the
implicit join: the filter narrows the eligible records and the ordered
selection chooses among the survivors exactly as an explicit lookup would.

### Terminology

<a id="req-0112"></a>

**REQ-0112.** The lookup's dataset is the relation read. For a named
intermediate, `dataset: SELF` instead reads the spec's completed derived
rows. The current row is
the output row (or grouped-row candidate) the match runs for. Match fields
are the `key` columns; match values are what the current row supplies for
them ([REQ-0115](lookup.md#req-0115)). Eligible records are the dataset
records surviving `filter`.

<a id="req-0113"></a>

**REQ-0113.** A lookup `id` shares one naming list with dataset
identifiers, other lookup ids, and the output `domain`. A collision fails
as `duplicate_identifier`.

<a id="req-0114"></a>

**REQ-0114.** A named lookup declares `id` and `dataset`; `key` is
optional. The schema requires `id` and `dataset`: omitting either fails as
`missing_required_field` with no requirement attached. An omitted `key` is
inferred from the applicable output keys ([REQ-0153](lookup.md#req-0153)).
State `key` only when the intended match differs from the inferred match.

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

The lookup above matches on the applicable output keys. The form below
states the same match explicitly:

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

<a id="req-1248"></a>

**REQ-1248.** A named intermediate must narrow, derive, or reshape its
dataset, or require that every read selects a record. An intermediate that
declares nothing beyond `id`, `dataset`, and `no_match: null` reads exactly
what the implicit join reads ([REQ-0111](lookup.md#req-0111)) and fails as
`rename_only_intermediate`: read the input dataset directly instead of
aliasing it.

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

<a id="req-0115"></a>

**REQ-0115.** `key` states the match as pairs, each a column of the
lookup's dataset and the current-row match value it must equal. A column
name, or a list of column names, pairs each column with the same-named
current-row value. A mapping pairs each column, written as the mapping key,
with the match value written beside it: a variable or an expression
([REQ-1259](lookup.md#req-1259)). A mapping may pair a column with the
same-named value. Pair order does not change the result.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: {TESTCD: LBTESTCD, SEX: SEX}
```

Here the current-row `LBTESTCD` matches the limit table's `TESTCD` column;
the list `[TESTCD, SEX]` would have matched `TESTCD` against a current-row
`TESTCD` that does not exist.

A written `key` names at least one pair. An empty list or mapping names no
match: a lookup fails as `invalid_field_type`, and an aggregate fails as
`missing_aggregate_keys` ([REQ-0140](lookup.md#req-0140)).

<a id="req-0116"></a>

**REQ-0116.** Every `key` column must exist in the lookup's dataset.
Otherwise fail as `unknown_field`.

<a id="req-0117"></a>

**REQ-0117.** Every match value must read a known current-row value: a
variable must name one, and every identifier an expression reads must be
one. Otherwise fail as `unknown_field`. `SELF` names donor rows and is
never a current-row scope. A match value written `SELF.field` therefore
names no current-row value and fails the same way. When the current row
carries `field`, the diagnostic suggests that bare name.

<a id="req-0118"></a>

**REQ-0118.** Each source/key pair must be mutually comparable under
[REQ-0005](../values/types.md#req-0005). The match converts no operand. A pair that cannot compare fails
as `incompatible_input_type` under [REQ-0323](../values/types.md#req-0323), reporting both declared
types. [REQ-0517](../storage/ingestion.md#req-0517) gives an undeclared field of a typeless container the type
`str`; a key typed on one side and defaulted on the other needs repair,
not a wider comparison.

<a id="req-1259"></a>

**REQ-1259.** A match value in a `key` mapping may be an expression instead
of a variable.
The expression is evaluated against the current row and its value is the
match operand for that position; a result that is missing matches nothing,
exactly as a missing variable does ([REQ-0131](lookup.md#req-0131)). The
pair's [REQ-0118](lookup.md#req-0118) comparison uses the expression's
statically known result type as the source side. For this comparison, the
following table is the complete set of operations with a static type. The
type is the operation's ordinary non-missing result, before any local
`missing`, `invalid`, or `no_match` replacement. The operation contracts
define those results; this table fixes which of them are checked before
matching, independent of the runtime implementation.

| Static comparison type | Operations |
| --- | --- |
| Referenced variable's type | `source` |
| Declared YAML scalar's type (`str`, `int`, `float`, or `bool`); a missing literal has no static type | `literal` |
| `date` | `date_impute`, `to_date` |
| `datetime` | `datetime_impute` |
| `int` | `date_diff`, `rank`, `row_number`, `study_day`, `to_epoch_day` |
| `float` | `round_half_away_from_zero` |
| `str` | `baseline_flag`, `cut`, `date_precision`, `datetime_precision`, `str_case`, `str_concat`, `str_extract`, `str_pad`, `str_template` |
| `bool` | `str_contains` |
| Numeric (`int` or `float`) | `compute` |

For `compute`, either possible result type compares with `int` and `float`
under [REQ-0005](../values/types.md#req-0005); `float` represents that
numeric comparison class in a validation diagnostic, without changing the
computed value. Every other expression operation states no static type for
this check, even when a particular invocation's inputs could reveal one.
For example, `mapping`, `case`, `greatest`, and `least` select values whose
types depend on their inputs. Such pairs are judged by whether their match
values compare at run time. A non-missing local replacement is likewise
subject to run-time comparability when it is used as a match operand.

<a id="req-0119"></a>

**REQ-0119.** `order_by` and `keep` are declared together or not at all.
Declaring one without the other fails as `unpaired_fields`.

<a id="req-0120"></a>

**REQ-0120.** A lookup's `dataset` must be declared in `input`, except that a
named intermediate may use `SELF` when the spec declares row templates. `SELF`
is reserved for this purpose and cannot also name an input. It contains rows
from completed earlier templates during row construction, and all completed
rows during column derivation. The current template's unfinished rows are
never eligible. A read from the first template fails as `phase_boundary`.
The donor fields are output columns derived in every row template; a column
derived only in the later column phase is unavailable to `SELF`. The order of
donors follows row construction order. `SELF` may be read only through a named
intermediate, not as a directly qualified source.

For `SELF`, `filter` and `order_by` may name its donor fields either bare or
as `SELF.field`. Bare donor fields in `filter` are not correlated current-row
references. All other lookup rules, including matching, missing values,
ordering, and `columns`, apply to `SELF` as they do to input datasets.

A `key` match value still reads the current row
([REQ-0117](lookup.md#req-0117)): a bare name there is the current row's
column, not a donor field. Row templates reading one `SELF` intermediate
can each supply a match value from their own derivations. Below, an
expected-record template derives its window and reads `COVER` to find a
completed row already in that window; another template deriving
`AVISIT: {literal: WEEK 4}` reads the same intermediate for its own window.

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

For input datasets, `order_by` names qualified fields of that dataset only; `columns`
lists bare field names. Every `filter` field is qualified. The lookup's
own qualifier reads a candidate donor record. A different qualifier may
read the current row's driver: root `base` (or the sole input)
without explicit rows, or the enclosing `rows[].dataset` during row
construction. During column derivation it must be the driver of every
row template that the expression can evaluate on. The row driver may be an
eligible named intermediate under [REQ-1262](../execution/rows.md#req-1262).
Other datasets, intermediates that are not the current driver, and
unqualified output names are not filter scopes.

The current-driver references are dependencies of each lookup read and
obey the existing phase and group-key rules. In a grouped template, the
qualified field must be a group key. Unknown fields fail as `unknown_field`;
when the bare field exists in the donor dataset, suggest its qualified
spelling. `order_by` remains donor-only even when `filter` is correlated.
When driver and donor use the same dataset identifier, that qualifier
always names the donor. Declare a separate input identifier for the same
resource when both sides need to be referenced explicitly.

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

The comparison restricts eligible donors; ordered selection chooses one.
`PRIOR.AVAL`, `PRIOR.ADT` and other reads share the chosen record, including
its missing fields. No rows are generated by the lookup.

<a id="req-0121"></a>

**REQ-0121.** A `between` declaration names one current-row `value` and
both `lower` and `upper` columns of the lookup's dataset; the schema
requires all three. A bound naming a column the dataset does not have,
or a `value` that is not a known variable, fails as `unknown_field`. The
value and both bounds must be mutually comparable: `int` and `float`
compare through [Numeric values](../values/numbers.md)'s numeric promotion and every other runtime type
must match exactly, with no operand converted. A mismatch fails as
`incomparable_range_types` before any record is compared, reporting the
three runtime types.

<a id="req-0122"></a>

**REQ-0122.** A declared `columns` list restricts which dataset columns
the lookup may read. Naming a column the dataset does not have fails as
`unknown_field`.

### Matching

<a id="req-0124"></a>

**REQ-0124.** An intermediate that yields nothing and declares no
`no_match` fails as `unmatched_key`, reporting the source values and the key
columns it sought.

<a id="req-0125"></a>

**REQ-0125.** A variable qualified by a lookup id (`DEATHEV.AEDECOD`)
reads the named column of the selected record in any field typed as
`variable`. The column must exist in the lookup's dataset -- and, when
`columns` is declared, be one of them -- or the read fails as
`unknown_field`.

<a id="req-0126"></a>

**REQ-0126.** During grouped row construction, every variable a lookup
matches on must be derived by the row template that reads the lookup.
The exceptions are the template's group keys and, for an ungrouped
template, the driver record's own fields.
[Execution lifecycle](../execution/lifecycle.md) orders row derivations before column derivation; a match value
available only in a later phase fails as `phase_boundary`.

<a id="req-0127"></a>

**REQ-0127.** More than one surviving record with no `order_by`/`keep` to
choose by is an unhandled multiple match: fail as
`multiple_matches`.

<a id="req-0128"></a>

**REQ-0128.** Both `between` endpoints are inclusive, and a record missing
a stated bound is ineligible rather than open-ended.

<a id="req-0129"></a>

**REQ-0129.** Yielding nothing is answered by the intermediate's `no_match`
handler, its absence policy. With `no_match` declared, every column reading
the intermediate receives its literal, and `no_match: null` answers with
missing. The answer is recorded under [Local handlers](../execution/handlers.md)'s `no_match` handler.
Without `no_match`, the read fails as `unmatched_key` ([REQ-0124](lookup.md#req-0124)).

<a id="req-0130"></a>

**REQ-0130.** A selected record whose value is missing differs from a
lookup that selected nothing. The first is a collected blank; the second
is an absent record. The absence policy answers only for an absent
record.

<a id="req-0131"></a>

**REQ-0131.** A missing match value is incomplete, not unmatched: it
yields nothing before any record is sought, and the absence policy
answers the same way.

<a id="req-0132"></a>

**REQ-0132.** A lookup `filter` identifier must name an available field in
one of the scopes admitted by [REQ-0120](lookup.md#req-0120), otherwise fail
as `unknown_field`. A filtered scalar source remains donor-only; correlated
predicates are supported by named intermediates.

<a id="req-0133"></a>

**REQ-0133.** A donor-only `filter` selects eligible records once per run.
A filter referencing the current driver is evaluated for each current row
against equality-matched donor records, before range narrowing and ordered
selection. Keep a candidate only when the predicate is TRUE; FALSE and
UNKNOWN exclude it. The donor dataset and equality index may be cached,
but a correlated eligibility result must never be reused for another row.
Normal predicate typing and missing-value semantics apply without coercion.

<a id="req-0134"></a>

**REQ-0134.** Surviving records match by equality on every source/key
pair, then narrow by `between`: a record is kept when `lower <= value`
and `value <= upper`.

<a id="req-0135"></a>

**REQ-0135.** With `order_by`/`keep`, the ordered first or last record is
chosen and remaining ties break by record order.

<a id="req-0136"></a>

**REQ-0136.** An input lookup's dataset is read once per run. The per-row match
selects among records already read; it never re-reads the dataset. A `SELF`
intermediate extends its donor pool after each completed row template and
invalidates its cached selection index at that boundary.

<a id="req-0138"></a>

**REQ-0138.** A named lookup's selected record is read by several columns
through lookup-qualified variables. Every column reading the same lookup
sees the same record: the match runs once per row and the selection is
shared.

<a id="req-1264"></a>

**REQ-1264.** A named lookup matches only when it is read. The match runs the
first time a column reads the lookup for a row, and the selection is shared
with every later read in that row ([REQ-0138](lookup.md#req-0138)). An
intermediate without `no_match` never fails
as `unmatched_key` on a row that did not read it.

<a id="req-0139"></a>

**REQ-0139.** Handler accounting follows [Local handlers](../execution/handlers.md): a `keep` that chose among
surviving records counts one `multiple_matches` handling, and a declared
`no_match` that answered an absence counts one `no_match` handling.

<a id="req-1185"></a>

**REQ-1185.** An intermediate may declare `derivations:`, a map of names to
derivations written in the same expression language as row-template
`derivations:`. The derivations evaluate in declaration order: a derivation
may read the dataset's stored fields plus the derivations declared before
it. A bare name reads the dataset's stored field or an earlier derived
name. A qualified name may read the intermediate's dataset, the current
input record under the same scope as a correlated `filter`
([REQ-0120](lookup.md#req-0120)), or, under
[REQ-1263](lookup.md#req-1263), another named intermediate. A reference to
an unrelated input dataset, a derivation declared later in the same map,
or an unknown field fails as `unknown_field`; a derived name
that shadows a stored column fails as `duplicate_derivation`.

Derivations that read only donor records are computed once per record.
When any derivation reads the current input record, the map evaluates for
each lookup's current record, and its augmented donor records, filters,
and match indexes cannot be reused for another current record. Every
column reading the intermediate still shares the same selection
([REQ-0138](lookup.md#req-0138)).

A derivation may use a window function. The window partitions the donor
records as augmented by every derivation declared before it, so its
`group_by`, `order_by`, and `filter` may read a stored field or an earlier
derived name, bare or dataset-qualified. A window's fields cannot read the
current input record directly: derive that value first and let the window
read the derived name. Base-record order is the final tie-break.

The derived values augment each donor record before `filter`, matching,
`order_by` selection, and `columns` projection. A derived name may therefore
appear as a target-side `key`, as a dataset-qualified field in `filter` or
`order_by`, in `columns`, or in `verification.unique`, and behaves like a
stored field there. A correlated filter sees the augmented donor record and
the current driver record together. A derivation that fails on a record fails
the run with the expression's condition at the derivation's path. A derivation
that yields missing contributes an ordinary missing value. That value cannot
match a key and may be excluded by a predicate.

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

<a id="req-1263"></a>

**REQ-1263.** An intermediate derivation may read a column of another named
intermediate through that intermediate's id. The read runs the other
intermediate's match for the donor record being augmented, not for an
output row. Every match value, `between` value, and correlated `filter`
field that the match reads comes from that donor record under
[REQ-1185](lookup.md#req-1185). Each is written as a bare name, or as a
name qualified by the reading intermediate's dataset. Either form names a
stored field or a derivation declared before the reading one. A name the
donor record cannot supply fails as `unknown_field`. The read column must
exist in the other intermediate's dataset or derivations and, when that
intermediate declares `columns`, be one of them
([REQ-0125](lookup.md#req-0125)).

Filtering, matching, range narrowing, ordered selection, and the absence
policy apply exactly as for any other read of that intermediate. It is
selected once per donor record, and every derivation of that record that
reads it shares the selection ([REQ-0138](lookup.md#req-0138)). A read that
fails, such as an intermediate without `no_match` that selects nothing,
fails the run with that intermediate's condition at that intermediate's
path, as a row reading it would. The read adds no current-row dependency and never changes
the number of donor records ([REQ-0146](lookup.md#req-0146)).

The other intermediate must read an `input` dataset. A `SELF` intermediate
has no completed rows while donor records are augmented, so reading one
fails as `phase_boundary`. Intermediates whose derivations read each
other, directly or through a chain, fail as `dependency_cycle`. A window's
fields read only the donor records being partitioned. A read of another
intermediate in a window's `group_by`, `order_by`, `filter`, or value field
fails as `unknown_field`: derive the value first and let the window read
the derived name.

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

Each laboratory record reads its own supplemental endpoint qualifier, and
the window ranks the records of each subject and test on it.

### Intermediate uniqueness checks

<a id="req-1245"></a>

**REQ-1245.** An intermediate may declare a `verifications:` list of
`unique: [...]` checks. Each check asserts that its nonempty
column combination is unique across the intermediate's filtered donor records:

```yaml
- id: DS_EOS
  dataset: DS
  filter: "DS.DSCAT = 'DISPOSITION EVENT' AND DS.DSDECOD <> 'SCREEN FAILURE'"
  verifications:
    - unique: [STUDYID, USUBJID]
```

The mapping form `unique: {columns: [STUDYID, USUBJID], id: donor-key}`
names a check when an explicit ID is useful. Both forms test the same donor
records.

The check runs over the source-only filtered donor records with
`derivations:` computed. Input-backed checks run before any row is built;
`SELF` checks run after each completed template. A lookup whose
key is verified unique needs neither `keep` nor `order_by`:
[REQ-0127](lookup.md#req-0127) already rejects multiple surviving
matches without a selection rule. A repeated combination fails the run
as `duplicate_intermediate_records` at the
`intermediates[i].verifications[j].unique` path. Each check may declare an
optional `id`, governed by REQ-0374 and REQ-0398. There is no warning
severity, so a duplicate can never resolve
ambiguously. Each `unique` column must name a stored field or a derived
name, or fail as `unknown_field`. A `filter` or `derivations:` entry that
fails to materialize fails the run here as well: the verification is
load-bearing, so its failure cannot wait for a selection that may never
happen. A `filter` that references the current
input record is evaluated per record and admits no single run-wide donor set,
so it cannot combine with `verifications:` and fails validation.
A derivation that reads the current input record likewise cannot combine
with `verifications:` and fails as
`correlated_derivation_with_unique_verification`.
`keep` stays a positional selection requiring `order_by`; an unordered
`keep: first` is not an exactly-one assertion.

### Aggregates over a qualified relation

<a id="req-0140"></a>

**REQ-0140.** An aggregate whose expression reads a qualified dataset
relation matches on `key` pairs ([REQ-0115](lookup.md#req-0115)). An
omitted `key` is inferred from the applicable output keys
([REQ-0153](lookup.md#req-0153)). With no pairs at all the aggregate names
no match and fails as `missing_aggregate_keys`.

<a id="req-0141"></a>

**REQ-0141.** An aggregate's declared `key` columns must exist in the
relation and its match values must be known, or fail as
`unknown_field`. A pair that cannot compare fails under [REQ-0004](../values/types.md#req-0004).

<a id="req-0142"></a>

**REQ-0142.** A grouped-row aggregate reads its own input group and
declares no key pairs: the group is the match.

### Failures share one vocabulary

<a id="req-0143"></a>

**REQ-0143.** `key` and `intermediate_key` name the fields a lookup matched on
and the values it matched them with; `keys` names the output row the
failure belongs to. An unmatched key and an unhandled multiple match
report the match the same way, and neither renames the other's fields.

<a id="req-0144"></a>

**REQ-0144.** `unmatched_key` carries the sought source values; a column
that cannot identify which row it sought cannot explain its absence.

<a id="req-0145"></a>

**REQ-0145.** During grouped row construction, more than one surviving
record with no `order_by`/`keep` fails as `multiple_matches` at the
join phase, before any column is derived.

<a id="req-0146"></a>

**REQ-0146.** A lookup never changes the row count. It answers one value
per current row: the selected record's column, the `no_match` literal,
or an `unmatched_key` failure.

<a id="req-0147"></a>

**REQ-0147.** The absence policy applies per column read. Every column
reading an absent lookup receives the `no_match` literal independently;
one column's handling never answers for another.

<a id="req-0148"></a>

**REQ-0148.** A `filter` on a construct with no records to select
among -- an output column, a chosen lookup record, a group key: fail
as `prohibited_construct`.

### Review

<a id="req-0149"></a>

**REQ-0149.** Validation reports the source/key pairs for every lookup
and every implicit join. A reviewer sees exactly the match the engine
performs, the type each side declares, whether the pairs were inferred
([REQ-0150](lookup.md#req-0150)) or declared, and the absence policy that answers a miss.

### The implicit join

<a id="req-0150"></a>

**REQ-0150.** The applicable keys are the output `keys`, in output-key
order, that the right-side dataset also carries. The implicit join
matches the current row's values of those keys against the same-named
columns of the dataset. At least one applicable key is required; the
match is left-row preserving, and a current row with no right-side
match yields a missing result.

<a id="req-0151"></a>

**REQ-0151.** An inferred key must compare equal on both sides. [REQ-0004](../values/types.md#req-0004)
performs no implicit conversion, so an applicable key whose left and
right types are not mutually comparable fails as
`incompatible_input_type`, reporting both types.

<a id="req-0152"></a>

**REQ-0152.** With no applicable key the intended match is unclear: the
read fails as `no_applicable_keys`, and the author states the match with
a named intermediate declaring its `key`. The same explicit
form serves whenever the intended keys differ from the applicable
output keys.

<a id="req-0153"></a>

**REQ-0153.** A named lookup or a qualified aggregate may omit `key`: the
omitted key is the applicable keys of [REQ-0150](lookup.md#req-0150) -- the
output `keys`, in output-key order, that the right-side dataset also carries.
With no applicable key the read fails as `no_applicable_keys`.

### Row construction reads through the same join

<a id="req-0156"></a>

**REQ-0156.** During ungrouped row construction, a scalar source qualified
with another input dataset joins that dataset on the applicable keys
([REQ-0150](lookup.md#req-0150)). Each key matches the same-named field of the row template's
input record. Each retained input record builds one candidate row
([REQ-0036](../execution/rows.md#req-0036)). The input record's fields are the only single values the match
can read. Values produced only by column derivation are not available
then. A named intermediate states the same match with declared `key`
pairs. Its match values have the same availability:
input-record fields or columns in the same row template. The join binds
one value per row. Later derivations in the same row template read that
value through the bound column. A row-phase `compute` still names no
qualified cross-dataset identifier ([REQ-0410](computation.md#req-0410)).

<a id="req-0157"></a>

**REQ-0157.** During grouped row construction, the same join matches each
applicable key against the candidate's group-key value. Every applicable
key must be a group key of the row template ([REQ-0037](../execution/rows.md#req-0037)). A key the group
does not carry varies within the group and names no single match value:
fail as `ungrouped_driver_field` ([REQ-0067](../execution/rows.md#req-0067)). A key no input record
carries fails as `unknown_field` ([REQ-0103](../specification/binding.md#req-0103)), in either mode.

### Type behavior

<a id="req-0305"></a>

**REQ-0305.** An intermediate's `key` requires each match value and its
paired key column to have the same comparable type.

### Interface behavior

<a id="req-1048"></a>

**REQ-1048.** The `intermediate_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `intermediate_class.id` | Name through which the looked-up record is read. |
| `intermediate_class.dataset` | Declared input dataset, or `SELF` for completed derived rows ([REQ-0120](lookup.md#req-0120)). |
| `intermediate_class.key` | Dataset columns, each paired with its current-row match value ([REQ-0115](lookup.md#req-0115)); omit to match on the applicable output keys ([REQ-0150](lookup.md#req-0150)). |
| `intermediate_class.between` | Current-row value matched inclusively against the stated donor-record bounds ([REQ-1049](lookup.md#req-1049)). |
| `intermediate_class.filter` | Predicate selecting donor records; it may correlate with the current driver under REQ-0120. |
| `intermediate_class.order_by` | Terms ordering eligible records; declared with keep. |
| `intermediate_class.keep` | Ordered record to retain; declared with order_by. |
| `intermediate_class.columns` | Stored and derived columns the lookup may read; defaults to every available column. |
| `intermediate_class.derivations` | Per-record derivations over donor fields, the current input record, and records other named intermediates select for it ([REQ-1263](lookup.md#req-1263)); available to `key`, `filter`, `order_by`, and `columns`, and to source-only `verifications.unique` ([REQ-1185](lookup.md#req-1185)). |
| `intermediate_class.verifications` | Uniqueness checks over the filtered donor records ([REQ-1245](lookup.md#req-1245)). |
| `intermediate_class.no_match` | Value returned when the lookup yields nothing; without it, yielding nothing fails ([REQ-0124](lookup.md#req-0124)). |

<a id="req-1049"></a>

**REQ-1049.** The `record_between_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `record_between_class.value` | Value read from the current row. |
| `record_between_class.lower` | Donor-record field whose value is an inclusive lower bound; omit for no lower bound. |
| `record_between_class.upper` | Donor-record field whose value is an inclusive upper bound; omit for no upper bound. |

At least one of `lower`, `upper` is present; a `between` with neither bound
fails validation. Bounds are bare donor-record field identifiers.

<a id="req-1050"></a>

**REQ-1050.** The `intermediate_id` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `intermediate_id` | Identifier of a lookup, unique among dataset identifiers, other lookups, and the output domain. |

<a id="req-1051"></a>

**REQ-1051.** The `source_binding_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `source_binding_class.variable` | Variable to copy. |
| `source_binding_class.filter` | Predicate selecting the right-side records this source reads; [Lookup and joins](lookup.md) defines the selection. |
| `source_binding_class.absent` | Value used when the source variable or item is absent. |
| `source_binding_class.order_by` | Terms ordering eligible right-side matches; declared with keep ([REQ-0119](lookup.md#req-0119)). |
| `source_binding_class.keep` | Ordered match to retain; declared with order_by ([REQ-0119](lookup.md#req-0119)). |

<a id="req-1052"></a>

**REQ-1052.** The `filtered_source` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `filtered_source` | A variable, or a variable with the predicate selecting the records it reads. |

<a id="req-1053"></a>

**REQ-1053.** The `filtered_source_class` fields have these meanings:

| Field | Meaning |
| --- | --- |
| `filtered_source_class.variable` | Variable to copy. |
| `filtered_source_class.filter` | Predicate selecting the right-side records this source reads; [Lookup and joins](lookup.md) defines the selection. |
