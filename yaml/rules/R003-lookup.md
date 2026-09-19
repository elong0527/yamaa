---
id: R003
title: Lookup
status: normative
applies_to: [lookups, expression.lookup, expression.aggregate, scalar.source]

---

# Lookup

## Intent

Read another dataset through a stated match. A plain scalar
`source: DATASET.COLUMN` joins that dataset on the applicable keys
whenever those keys are clear; an explicit `lookup:` states the match
itself for every case where the keys are unclear, differ from the
applicable output keys, or the read should be a reusable named lookup.
Keys are inferred only from the output `keys`, never invented: when no
applicable key exists, the author declares the match explicitly.

Two explicit forms share one mechanism. A named `lookups:` entry selects
one record of a dataset for several columns to read. An inline `lookup:`
expression looks up one value for one column. Both match, narrow, choose,
and answer absence the same way.

R015 (record lookup) is retired: this rule is its replacement. The old
`mapping_from` is retired into the inline `lookup:` expression.

## Boundaries

This rule owns the lookup declaration, the match, and the absence policy.
R007 owns the inline `lookup:` operation's mechanics: each source and its
key column must have the same comparable type (R007-21), `value` must name
a dataset column (R007-36), and the two lists must pair (R007-48). The
incompatible-input condition is R007-38 wherever a pair fails to compare.
R008 owns handler accounting: a `keep` that chose among surviving records
counts `multiple_matches`, and a declared `missing:` that answered an
absence counts `missing`. R013 owns aggregates; an aggregate over a
qualified relation declares its key pairs here (R003-30), and an
aggregate expression that violates R013 fails there (R013-38 at
`...aggregate.expr`). R002 owns binding: a qualified source naming no
declared dataset or lookup fails at binding as `unknown_field`. A
qualified source naming a real dataset with no applicable key fails as
`no_applicable_keys` (R003-42), because the implicit join has no stated
identity to match on. R001 owns dependency cycles: a column
that reads a lookup whose match depends on that column is a cycle, and
R001-18 makes the applicable keys of an implicit join dependencies of
the column that reads through it, so the keys are derived first.

## Declaration

**R003-1.** A dataset-qualified scalar source reads that dataset through
the implicit join: one value per current row, matched on the applicable
keys (R003-40), answering absence as a missing result. The qualifier
naming the current row's own driver is not a join: it reads the driver
record the row was constructed from.

```yaml
derivation:
  source: ADSL.TRTSDTM
```

A structured `source:` keeps its `filter` and `multiple_matches` on the
implicit join: the filter narrows the eligible records and
`multiple_matches` chooses among the survivors exactly as an explicit
lookup's would.

## Terminology

**R003-2.** The lookup's dataset is the relation read. The current row is
the output row (or grouped-row candidate) the match runs for. Match fields
are the `key` columns; match variables are the `source` values. Eligible
records are the dataset records surviving `filter`.

**R003-3.** A lookup `id` shares one namespace with dataset identifiers,
other lookup ids, and the output `domain`. A collision fails as
`duplicate_identifier`.

**R003-4.** A named lookup declares `id`, `dataset`, `source`, and `key`.
The schema requires all four: omitting `key` (or `source`) fails as
`missing_required_field` with no requirement attached, because the
contract is structural -- the old positional-unpaired condition has no
separate requirement number.

```yaml
lookups:
  - id: DEATHEV
    dataset: AE
    source: [STUDYID, USUBJID]
    key: [STUDYID, USUBJID]
    filter: "AE.AEOUT = 'FATAL'"
    order_by: [AE.ASTDT]
    keep: last
```

**R003-5.** `source` and `key` pair by position, have equal length, and
are both non-empty. Otherwise the lookup names no key and fails as
`source_key_length_mismatch`.

**R003-6.** Every `key` column must exist in the lookup's dataset.
Otherwise fail as `unknown_field`.

**R003-7.** Every `source` variable must be a known current-row value.
Otherwise fail as `unknown_field`.

**R003-8.** Each source/key pair must be mutually comparable under
R007-31. The match converts no operand. A pair that cannot compare fails
as `incompatible_input_type` under R007-38, reporting both declared
types. R014-4 gives an undeclared field of a typeless container the type
`str`; a key typed on one side and defaulted on the other needs repair,
not a wider comparison.

**R003-9.** `order_by` and `keep` are declared together or not at all.
Declaring one without the other fails as `unpaired_fields`.

**R003-10.** `filter`, `order_by`, and `columns` name records and fields
of the lookup's own dataset only, and `dataset` must be declared in
`input`. Anything else fails as `unknown_field`.

**R003-11.** A `between` declaration names one current-row `value` and
both `lower` and `upper` columns of the lookup's dataset; the schema
requires all three. A bound naming a column the dataset does not have,
or a `value` that is not a known variable, fails as `unknown_field`. The
value and both bounds must be mutually comparable: `int` and `float`
compare through R010's numeric promotion and every other runtime type
must match exactly, with no operand converted. A mismatch fails as
`incomparable_range_types` before any record is compared, reporting the
three runtime types.

**R003-12.** A declared `columns` list restricts which dataset columns
the lookup may read. Naming a column the dataset does not have fails as
`unknown_field`.

**R003-13.** `strict: true` together with `missing:` is a contradiction --
a failing absence and a returned literal -- and fails as
`conflicting_absent_policy`.

## Matching

**R003-14.** With `strict: true`, a lookup that yields nothing fails as
`unmatched_key`, reporting the source values and the key columns it
sought.

**R003-15.** A variable qualified by a lookup id (`DEATHEV.AEDECOD`)
reads the named column of the selected record in any field typed as
`variable`. The column must exist in the lookup's dataset -- and, when
`columns` is declared, be one of them -- or the read fails as
`unknown_field`.

**R003-16.** During grouped row construction, every variable a lookup
matches on must be derived by the row template that reads the lookup.
R001 orders row derivations before column derivation; a match value
available only in a later phase fails as `phase_boundary`.

**R003-17.** More than one surviving record with no `order_by`/`keep` to
choose by is the unhandled multiple match this rule refuses: fail as
`multiple_matches`.

**R003-18.** Both `between` endpoints are inclusive, and a record missing
a stated bound is ineligible rather than open-ended.

**R003-19.** Yielding nothing has one policy with two settings. `strict:
true` fails as `unmatched_key` (R003-14). Otherwise every column reading
the lookup receives the `missing:` literal, which defaults to missing. A
declared `missing:` that answered an absence is recorded under R008's
`missing` handler.

**R003-20.** A selected record whose value is missing differs from a
lookup that selected nothing. The first is a collected blank; the second
is an absent record. The absence policy answers only for an absent
record.

**R003-21.** A missing match value is incomplete, not unmatched: it
yields nothing before any record is sought, and the absence policy
answers the same way.

**R003-22.** A `filter` identifier, or any referenced name, that names no
field of the lookup's dataset fails as `unknown_field`.

**R003-23.** `filter` selects the eligible records once per run.
Eligibility does not vary by row; the per-row match starts from the same
eligible set every time.

**R003-24.** Surviving records match by equality on every source/key
pair, then narrow by `between`: a record is kept when `lower <= value`
and `value <= upper`.

**R003-25.** With `order_by`/`keep`, the ordered first or last record is
chosen and remaining ties break by record order.

**R003-26.** The lookup's dataset is read once per run. The per-row match
selects among records already read; it never re-reads the dataset.

**R003-27.** An inline `lookup:` expression performs the same match,
narrow, choose, and absence steps for one value:

```yaml
derivation:
  lookup:
    dataset: MEDDRA
    source: AE_RAW.AETERM
    key: LLTNAME
    value: PTNAME
    missing: NOT CODED
```

Its `filter`, `order_by`, `keep`, `between`, `missing`, and `strict`
behave exactly as the named form's. Its operation-level mechanics stay in
R007.

**R003-28.** A named lookup's selected record is read by several columns
through lookup-qualified variables. Every column reading the same lookup
sees the same record: the match runs once per row and the selection is
shared.

**R003-29.** Handler accounting follows R008: a `keep` that chose among
surviving records counts one `multiple_matches` handling, and a declared
`missing:` that answered an absence counts one `missing` handling.

## Aggregates over a qualified relation

**R003-30.** An aggregate whose expression reads a qualified dataset
relation declares the key pairs it matches on: `source` and `key` are
both required, pair by position, and are non-empty. Otherwise fail as
`missing_aggregate_keys`.

**R003-31.** An aggregate's declared `key` columns must exist in the
relation and its `source` variables must be known, or fail as
`unknown_field`. A pair that cannot compare fails under R007-19.

**R003-32.** A grouped-row aggregate reads its own driver group and
declares no key pairs: the group is the match.

## Failures share one vocabulary

**R003-33.** `key` and `lookup_key` name the fields a lookup matched on
and the values it matched them with; `keys` names the output row the
failure belongs to. An unmatched key and an unhandled multiple match
report the match the same way, and neither renames the other's fields.

**R003-34.** `unmatched_key` carries the sought source values; a column
that cannot identify which row it sought cannot explain its absence.

**R003-35.** During grouped row construction, more than one surviving
record with no `order_by`/`keep` fails as `multiple_matches` at the
join phase, before any column is derived.

**R003-36.** A lookup never changes the row count. It answers one value
per current row: the selected record's column, the `missing:` literal,
or a `strict:` failure.

**R003-37.** The absence policy applies per column read. Every column
reading an absent lookup receives the `missing:` literal independently;
one column's handling never answers for another.

**R003-38.** A `filter` on a construct with no records to select
among -- an output column, a chosen lookup record, a group key: fail
as `prohibited_construct`.

## Rationale

An expression returns one value. Before this rule, three mechanisms
reached another dataset -- the implicit join, `record_lookups`, and
`mapping_from` -- with three key derivations and three absence
vocabularies. A reviewer could not see that two columns reading "the
same" record agreed, and an edit to one key statement and not the other
broke the agreement silently.

The unification keeps the implicit join where the match is already
stated: the output `keys` name the row's identity, so a plain
`source: DATASET.COLUMN` matching on the applicable keys says nothing
twice. `record_lookups` and `mapping_from` become the one explicit
`lookup` for everything else -- an unclear key, a key that differs from
the applicable output keys, or a reusable named read -- so the
declaration a reviewer reads is the match the engine runs, whichever
form states it.

## Review

**R003-39.** Validation reports the source/key pairs for every lookup
and every implicit join. A reviewer sees exactly the match the engine
performs, the type each side declares, whether the pairs were inferred
(R003-40) or declared, and the absence policy that answers a miss.

## The implicit join

**R003-40.** The applicable keys are the output `keys`, in output-key
order, that the right-side dataset also carries. The implicit join
matches the current row's values of those keys against the same-named
columns of the dataset. At least one applicable key is required; the
match is left-row preserving, and a current row with no right-side
match yields a missing result.

**R003-41.** An inferred key must compare equal on both sides. R007-19
performs no implicit conversion, so an applicable key whose left and
right types are not mutually comparable fails as
`incompatible_input_type`, reporting both types.

**R003-42.** With no applicable key the intended match is unclear: the
read fails as `no_applicable_keys`, and the author states the match with
an explicit `lookup:` naming its `source`/`key` pairs. The same explicit
form serves whenever the intended keys differ from the applicable
output keys or the read should be a reusable named lookup.


## Errors

The failure vocabulary, in the order the requirements introduce it:
`no_applicable_keys` (R003-42), `duplicate_identifier` (R003-3),
`missing_required_field` (R003-4, schema phase, no requirement attached),
`source_key_length_mismatch` (R003-5), `unknown_field` (R003-6, R003-7,
R003-10, R003-12, R003-15, R003-22, R003-31), `incompatible_input_type`
(R003-8, R003-41), `unpaired_fields` (R003-9), `incomparable_range_types`
(R003-11), `conflicting_absent_policy` (R003-13), `unmatched_key`
(R003-14), `phase_boundary` (R003-16), `multiple_matches` (R003-17,
R003-35), `missing_aggregate_keys` (R003-30), and `prohibited_construct`
(R003-38).
