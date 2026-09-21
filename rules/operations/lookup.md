---
id: operations/lookup
title: Lookup and joins
status: normative
---

# Lookup and joins

## Purpose

Match declared keys, narrow records, select a result, and answer absence.

## Scope and dependencies

This contract owns the requirements below. Related contracts:

- [Local handlers](../execution/handlers.md).
- [Execution lifecycle](../execution/lifecycle.md).
- [Numeric computation](computation.md).
- [Expression evaluation](expressions.md).
- [Name binding](../specification/binding.md).
- [Source ingestion](../storage/ingestion.md).
- [Types and conversion](../values/types.md).


## Requirements

### Declaration

<a id="req-0111"></a>

**REQ-0111.** A dataset-qualified scalar source reads that dataset through
the implicit join: one value per current row, matched on the applicable
keys ([REQ-0150](lookup.md#req-0150)), answering absence as a missing result. The qualifier for
this row template's input dataset is not a join. It reads the input record
that built the row.

```yaml
derivation:
  source: ADSL.TRTSDTM
```

A structured `source:` keeps its `filter` and `multiple_matches` on the
implicit join: the filter narrows the eligible records and
`multiple_matches` chooses among the survivors exactly as an explicit
lookup would.

### Terminology

<a id="req-0112"></a>

**REQ-0112.** The lookup's dataset is the relation read. The current row is
the output row (or grouped-row candidate) the match runs for. Match fields
are the `key` columns; match variables are the `key_base` values. Eligible
records are the dataset records surviving `filter`.

<a id="req-0113"></a>

**REQ-0113.** A lookup `id` shares one namespace with dataset identifiers,
other lookup ids, and the output `domain`. A collision fails as
`duplicate_identifier`.

<a id="req-0114"></a>

**REQ-0114.** A named lookup declares `id` and `dataset`; `key_base` and
`key` are optional. The schema requires `id` and `dataset`: omitting
either fails as `missing_required_field` with no requirement attached,
because the contract is structural. An omitted `key` is inferred from
the applicable output keys ([REQ-0153](lookup.md#req-0153)); an omitted `key_base` defaults to
the key names ([REQ-0154](lookup.md#req-0154)). State both lists only when the intended match
differs from the inferred match.

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

The lookup above matches on the applicable output keys; the form below
states the same match explicitly for a reviewer who should not have to
infer it:

```yaml
intermediates:
  - id: DEATHEV
    dataset: AE
    key_base: [STUDYID, USUBJID]
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

<a id="req-0115"></a>

**REQ-0115.** `key_base` and `key` pair by position, have equal length, and
are both non-empty -- after inference ([REQ-0153](lookup.md#req-0153)) and defaulting ([REQ-0154](lookup.md#req-0154))
have run. Otherwise the lookup names no key and fails as
`source_key_length_mismatch`.

<a id="req-0116"></a>

**REQ-0116.** Every `key` column must exist in the lookup's dataset.
Otherwise fail as `unknown_field`.

<a id="req-0117"></a>

**REQ-0117.** Every `key_base` variable must be a known current-row value.
Otherwise fail as `unknown_field`.

<a id="req-0118"></a>

**REQ-0118.** Each source/key pair must be mutually comparable under
[REQ-0005](../values/types.md#req-0005). The match converts no operand. A pair that cannot compare fails
as `incompatible_input_type` under [REQ-0323](../values/types.md#req-0323), reporting both declared
types. [REQ-0517](../storage/ingestion.md#req-0517) gives an undeclared field of a typeless container the type
`str`; a key typed on one side and defaulted on the other needs repair,
not a wider comparison.

<a id="req-0119"></a>

**REQ-0119.** `order_by` and `keep` are declared together or not at all.
Declaring one without the other fails as `unpaired_fields`.

<a id="req-0120"></a>

**REQ-0120.** `filter`, `order_by`, and `columns` name records and fields
of the lookup's own dataset only, and `dataset` must be declared in
`input`. Anything else fails as `unknown_field`.

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

<a id="req-0123"></a>

**REQ-0123.** `strict: true` together with `missing:` is a contradiction --
a failing absence and a returned literal -- and fails as
`conflicting_absent_policy`.

### Matching

<a id="req-0124"></a>

**REQ-0124.** With `strict: true`, a lookup that yields nothing fails as
`unmatched_key`, reporting the source values and the key columns it
sought.

<a id="req-0125"></a>

**REQ-0125.** A variable qualified by a lookup id (`DEATHEV.AEDECOD`)
reads the named column of the selected record in any field typed as
`variable`. The column must exist in the lookup's dataset -- and, when
`columns` is declared, be one of them -- or the read fails as
`unknown_field`.

<a id="req-0126"></a>

**REQ-0126.** During grouped row construction, every variable a lookup
matches on must be derived by the row template that reads the lookup --
except the template's group keys, which are known while rows are built
(and, for an ungrouped template, the driver record's own fields).
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

**REQ-0129.** Yielding nothing has one policy with two settings. `strict:
true` fails as `unmatched_key` ([REQ-0124](lookup.md#req-0124)). Otherwise every column reading
the lookup receives the `missing:` literal, which defaults to missing. A
declared `missing:` that answered an absence is recorded under [Local handlers](../execution/handlers.md)'s
`missing` handler.

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

**REQ-0132.** A `filter` identifier, or any referenced name, that names no
field of the lookup's dataset fails as `unknown_field`.

<a id="req-0133"></a>

**REQ-0133.** `filter` selects the eligible records once per run.
Eligibility does not vary by row.

<a id="req-0134"></a>

**REQ-0134.** Surviving records match by equality on every source/key
pair, then narrow by `between`: a record is kept when `lower <= value`
and `value <= upper`.

<a id="req-0135"></a>

**REQ-0135.** With `order_by`/`keep`, the ordered first or last record is
chosen and remaining ties break by record order.

<a id="req-0136"></a>

**REQ-0136.** The lookup's dataset is read once per run. The per-row match
selects among records already read; it never re-reads the dataset.

<a id="req-0137"></a>

**REQ-0137.** An inline `lookup:` expression performs the same match,
narrow, choose, and absence steps for one value:

```yaml
derivation:
  lookup:
    dataset: MEDDRA
    key_base: AE_RAW.AETERM
    key: LLTNAME
    value: PTNAME
    missing: NOT CODED
```

Its `filter`, `order_by`, `keep`, `between`, `missing`, and `strict`
behave exactly as the named form's, and its `key_base`/`key` follow the
same omission rules ([REQ-0153](lookup.md#req-0153), [REQ-0154](lookup.md#req-0154)). Its operation-level mechanics
are defined by this lookup contract; registry dispatch follows
[Expression evaluation](expressions.md).

<a id="req-0138"></a>

**REQ-0138.** A named lookup's selected record is read by several columns
through lookup-qualified variables. Every column reading the same lookup
sees the same record: the match runs once per row and the selection is
shared.

<a id="req-0139"></a>

**REQ-0139.** Handler accounting follows [Local handlers](../execution/handlers.md): a `keep` that chose among
surviving records counts one `multiple_matches` handling, and a declared
`missing:` that answered an absence counts one `missing` handling.

### Aggregates over a qualified relation

<a id="req-0140"></a>

**REQ-0140.** An aggregate whose expression reads a qualified dataset
relation matches on key pairs: `key_base` and `key` pair by position and
are non-empty after inference ([REQ-0153](lookup.md#req-0153)) and defaulting ([REQ-0154](lookup.md#req-0154)) have
run. An omitted `key` is inferred from the applicable output keys; an
omitted `key_base` defaults to the key names. With no pairs at all the
aggregate names no match and fails as `missing_aggregate_keys`.

<a id="req-0141"></a>

**REQ-0141.** An aggregate's declared `key` columns must exist in the
relation and its `key_base` variables must be known, or fail as
`unknown_field`. A pair that cannot compare fails under [REQ-0004](../values/types.md#req-0004).

<a id="req-0142"></a>

**REQ-0142.** A grouped-row aggregate reads its own input group and
declares no key pairs: the group is the match.

### Failures share one vocabulary

<a id="req-0143"></a>

**REQ-0143.** `key` and `lookup_key` name the fields a lookup matched on
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
per current row: the selected record's column, the `missing:` literal,
or a `strict:` failure.

<a id="req-0147"></a>

**REQ-0147.** The absence policy applies per column read. Every column
reading an absent lookup receives the `missing:` literal independently;
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
an explicit `lookup:` naming its `key_base`/`key` pairs. The same explicit
form serves whenever the intended keys differ from the applicable
output keys or the read should be a reusable named lookup.

<a id="req-0153"></a>

**REQ-0153.** A named lookup, an inline `lookup:`, or a qualified
aggregate may omit `key`: the omitted key is the applicable keys of
[REQ-0150](lookup.md#req-0150) -- the output `keys`, in output-key order, that the right-side
dataset also carries. The inference is the same one the implicit join
uses, so a lookup that omits `key` matches exactly as the implicit join
would. With no applicable key the read fails as `no_applicable_keys`.

<a id="req-0154"></a>

**REQ-0154.** A lookup may omit `key_base`: the omitted source defaults to
the (possibly inferred) key names, matching each key column against the
same-named current-row value. `key_base` is stated only when a key column
is matched against a differently named current-row value.

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key_base: [LBTESTCD, SEX]
    key: [TESTCD, SEX]
```

Here the current-row `LBTESTCD` matches the limit table's `TESTCD`
column; omitting `key_base` would have matched `TESTCD` against a
current-row `TESTCD` that does not exist.

<a id="req-0155"></a>

**REQ-0155.** A written `key_base` must not simply repeat `key`: naming
the same columns on both sides states the [REQ-0154](lookup.md#req-0154) default twice, and the
two statements can then drift apart under edit. A `key_base` equal to
its `key` fails as `redundant_key_base`; the author omits it instead.
The requirement is on what the author wrote: the `key_base` [REQ-0154](lookup.md#req-0154)
supplies is never redundant.

### Row construction reads through the same join

<a id="req-0156"></a>

**REQ-0156.** During ungrouped row construction, a scalar source qualified
with another input dataset joins that dataset on the applicable keys
([REQ-0150](lookup.md#req-0150)). Each key matches the same-named field of the row template's
input record. Each retained input record builds one candidate row
([REQ-0036](../execution/rows.md#req-0036)). The input record's fields are the only single values the match
can read. Values produced only by column derivation are not available
then. An explicit `lookup:` states the same match with declared
`key_base`/`key` pairs. Its match variables have the same availability:
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

**REQ-0305.** `lookup` requires each source and its positionally
corresponding key column to have the same comparable type.

### Interface behavior

<a id="req-1048"></a>

**REQ-1048.** The `intermediate_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `intermediate_class.id` | Name through which the looked-up record is read. |
| `intermediate_class.dataset` | Declared dataset the record is selected from. |
| `intermediate_class.key` | Dataset columns paired by position with key_base; omit to match on the applicable output keys ([REQ-0150](lookup.md#req-0150)). |
| `intermediate_class.key_base` | Current-row values paired by position with key; omit when they name the same columns as key. Must not repeat the key names ([REQ-0155](lookup.md#req-0155)). |
| `intermediate_class.between` | Current-row value matched inclusively against lower and upper dataset bounds. |
| `intermediate_class.filter` | Predicate selecting eligible records before matching. |
| `intermediate_class.order_by` | Terms ordering eligible records; declared with keep. |
| `intermediate_class.keep` | Ordered record to retain; declared with order_by. |
| `intermediate_class.columns` | Dataset columns the lookup may read; defaults to every dataset column. |
| `intermediate_class.missing` | Value returned when the lookup yields nothing; defaults to missing. |
| `intermediate_class.strict` | Fail when the lookup yields nothing. |

<a id="req-1049"></a>

**REQ-1049.** The `intermediate_between_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `intermediate_between_class.value` | Value read from the current row. |
| `intermediate_between_class.lower` | Dataset column whose value is an inclusive lower bound. |
| `intermediate_between_class.upper` | Dataset column whose value is an inclusive upper bound. |

<a id="req-1050"></a>

**REQ-1050.** The `intermediate_id` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `intermediate_id` | Identifier of a lookup, unique among dataset identifiers, other lookups, and the output domain. |

<a id="req-1051"></a>

**REQ-1051.** The `source_binding_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `source_binding_class.variable` | Variable to copy. |
| `source_binding_class.filter` | Predicate selecting the right-side records this source reads; [Lookup and joins](lookup.md) defines the selection. |
| `source_binding_class.missing` | Value used when the source variable or item is absent. |
| `source_binding_class.multiple_matches` | Rule for selecting one of several right-side matches. |

<a id="req-1052"></a>

**REQ-1052.** The `filtered_source` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `filtered_source` | A variable, or a variable with the predicate selecting the records it reads. |

<a id="req-1053"></a>

**REQ-1053.** The `filtered_source_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `filtered_source_class.variable` | Variable to copy. |
| `filtered_source_class.filter` | Predicate selecting the right-side records this source reads; [Lookup and joins](lookup.md) defines the selection. |

<a id="req-1054"></a>

**REQ-1054.** The `multiple_matches_class` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `multiple_matches_class.order_by` | Terms used to order eligible right-side matches. |
| `multiple_matches_class.keep` | Ordered match to retain. |

<a id="req-1055"></a>

**REQ-1055.** The `expressions.lookup` interface has the following meanings. Shape, defaults, and
structural constraints come from its schema declaration.

| Field | Meaning |
| --- | --- |
| `expressions.lookup.value` | Dataset column returned by the lookup. |
| `expressions.lookup.dataset` | Declared dataset containing the lookup table. |
| `expressions.lookup.key_base` | One or more current-row lookup values; omit when they name the same columns as key. Must not repeat the key names ([REQ-0155](lookup.md#req-0155)). |
| `expressions.lookup.key` | Dataset columns paired by position with key_base; omit to match on the applicable output keys ([REQ-0150](lookup.md#req-0150)). |
| `expressions.lookup.filter` | Predicate selecting eligible records before matching. |
| `expressions.lookup.between` | Current-row value matched inclusively against lower and upper dataset bounds. |
| `expressions.lookup.order_by` | Terms ordering eligible records; declared with keep. |
| `expressions.lookup.keep` | Ordered record to retain; declared with order_by. |
| `expressions.lookup.missing` | Value returned when the lookup yields nothing; defaults to missing. |
| `expressions.lookup.strict` | Fail when the lookup yields nothing. |
| `Result` | Looks up one value in a declared dataset by explicit key pairs. Key_base and key lists pair by position, have equal length, and form a unique combined dataset key. Pair order does not change the result. Either list may be omitted: an omitted key is inferred from the applicable output keys ([REQ-0150](lookup.md#req-0150)), and an omitted key_base defaults to the key names. Key_base must not repeat the key names ([REQ-0155](lookup.md#req-0155)). |

## Error conditions

<a id="req-0333"></a>

**REQ-0333.** `lookup` whose `key_base` and `key` lists differ in length must fail.

## Conformance examples

Representative specifications, input data, and expected outcomes:

- [negative-overlapping-windows](../../benchmarks/negative-overlapping-windows/README.md).
- [negative-mapping-duplicate-key](../../benchmarks/negative-mapping-duplicate-key/README.md).
- [negative-mapping-unpaired-key](../../benchmarks/negative-mapping-unpaired-key/README.md).
- [negative-mapping-partial-key](../../benchmarks/negative-mapping-partial-key/README.md).

The [execution manifest](../../benchmarks/execution-manifest.yaml) records
which fixtures execute. Grammar contracts additionally replay their shared
vectors. Static validation does not establish runtime parity.

## Rationale

Match declared keys, narrow records, select a result, and answer absence. Keeping this topic in one contract lets
other owners refer to it without defining a second policy.
