---
id: R015
title: Record Lookup
status: normative
applies_to: [root.record_lookups, record_lookup_class, expression.source,
  numeric_expression]

---

# Record lookup

## Intent

Select one record of another dataset once, and read several of its columns,
so that the values a specification takes from that record are known to have
come from the same one.

## Boundaries

This rule owns the `record_lookups` declaration: how a record is matched and
chosen, what its name means, and what an unmatched left row receives. R003
owns the implicit join a qualified source performs on its own, and R007 owns
`mapping_from` and the per-column `multiple_matches` relaxation. Neither
changes here: a record lookup reaches the same records by the same means and
differs only in being named once and read many times.

## Declaration

**R015-1.** Each entry of `record_lookups` names a record:

```yaml
record_lookups:
  - id: LASTEX
    dataset: EX
    filter: "EX.EXENDTC IS NOT NULL"
    order_by: [EX.EXENDTC, EX.EXSEQ]
    keep: last
```

**R015-2.** `id` is a name in the same namespace as dataset identifiers, so it
must not equal a dataset identifier, another record lookup's `id`, or the
output `domain`.

## Matching

**R015-3.** A record lookup matches its `dataset` where a derivation reads
it: against a constructed current row during column derivation, or against
the current candidate during grouped row construction:

1. **R015-4.** `filter` selects eligible records. It is a predicate over
   records of the record lookup's dataset only, evaluated exactly as R003
   evaluates the filter of a right-side reduction.
2. **R015-5.** Eligible records are matched. When `source` and `key` are
   declared, they pair by position and match by equality, exactly as
   `mapping_from` does under R007. When neither is declared, the applicable
   output keys match, exactly as R003 defines them, and at least one is
   required.
3. **R015-6.** `between`, when declared, narrows the equality-matched records
   as described below.
4. **R015-7.** When `order_by` and `keep` are declared, the remaining records
   are ordered by R007's order terms and `first` or `last` is retained;
   remaining ties are resolved by record order. When they are not declared,
   more than one surviving record fails.

**R015-8.** `source` and `key` are declared together or not at all, and so are
`order_by` and `keep`. String matches in either form use R019 equality.

**R015-8a.** A declared `source` and `key` pair must be mutually comparable
under R007-31, as `mapping_from`'s pairs are under R007-21, as `between`'s
operands are under R015-11, and as an inferred applicable key is under
R003-13a. No operand is converted to make any of the four match.

**R015-9.** During grouped row construction, every current-row variable used
for matching must be derived by that row template. R001 orders those row
derivations before the derivation that reads the lookup and rejects a
dependency on a value that will not exist until column derivation.

**R015-10.** A record lookup may also match by a closed range. Declaring
`between` adds one `value` the current row reads and `lower` and `upper`
columns of the lookup's dataset. A record is eligible when `lower <= value`
and `value <= upper`; both endpoints are inclusive.

**R015-11.** The value and both bounds must be mutually comparable under R007.
`int` and `float` may compare through R010's numeric promotion; every other
runtime type must be the same. No operand is converted implicitly to make the
comparison work.

**R015-12.** A missing `between.value` is an incomplete match, answered before
the right side is searched. A right-side record missing a stated bound is
ineligible, and a complete value with no eligible record is `unmatched`. This
is the interval join R003 names: the comparison is fixed, the bounds name
right-side columns, and the value names one current-row variable, so a match
against a table of irregular intervals is declared rather than re-expressed
as literals. `between.value` is a dependency of every column that reads the
lookup, exactly as a `source` variable is.

## Reading a record lookup

**R015-13.** A variable qualified by a record lookup `id` reads that column of
the selected record in any field typed as `variable`. R010 also permits the
same qualified form as an identifier inside a column-level
`numeric_expression`:

```yaml
- name: RFXENDTC
  type: date
  derivation:
    source: LASTEX.EXENDTC
- name: EXDOSE0
  type: float
  derivation:
    source: LASTEX.EXDOSE
- name: EXDOSE2
  type: float
  derivation:
    compute:
      expr: "2 * LASTEX.EXDOSE"
```

**R015-14.** The named column must exist in the record lookup's dataset. A
stored value carries the type R014 gives that field.

**R015-15.** A record lookup is not evaluated ahead of the columns that read
it. It resolves where they do. A record lookup's `filter` and `order_by` name
records of its own dataset and contribute no output-column dependency. Its
`source` and `between.value` variables do contribute dependencies, exactly as
a column using `mapping_from` depends on its source variables. R001 therefore
detects a cycle when a column reads a record lookup whose match depends
directly or indirectly on that column.

## When no record is selected

**R015-16.** Two conditions leave a left row with no record, and they stay
disjoint the way R008 keeps them disjoint for `mapping_from`: an incomplete
match value is answered before any record is looked for, and an unmatched key
is answered after.

**R015-17.** `incomplete` answers the first. A declared `source` or
`between.value` whose value is missing cannot be matched with anything, and
the default is `fail`, because a lookup that quietly returns nothing for an
uncollected match value reports an absent record that was never looked for.
Output keys are never missing, as R005 requires.

**R015-18.** `unmatched` answers the second: a complete match value that no
record carries. `missing` gives every column that reads the record lookup a
missing value, and `fail` rejects the run.

**R015-19.** Omitting `unmatched` keeps the behavior of the match the record
lookup performs, so replacing an existing derivation with a record lookup
never changes what an absent record does:

- **R015-20.** matching on output keys defaults to `missing`, because R003
  treats an absent right-side record as an ordinary missing enrichment;
- **R015-21.** matching on a declared `source` and `key` defaults to `fail`,
  because R007 makes an unmatched lookup key fatal unless the specification
  answers for it.

**R015-22.** A record lookup that matched a record whose value is missing is a
different case from one that matched nothing. The first is a collected blank
and the second is an absent record, and `unmatched` answers only for the
second.

## Rationale

An expression returns one value, so without a named record every expression
that reads another dataset reaches its own record. Two columns that must
describe one record -- a date and the sequence number identifying the record
it came from, a value and the unit it was measured in -- then state their
match twice and agree only by construction. A reviewer cannot see that
agreement, and an edit to one statement and not the other breaks it silently.
A record lookup states the match once and gives the chosen record a name, so
the columns that read it are plainly reading one record.

## Errors

- **R015-23.** A record lookup `id` equal to a dataset identifier, another
  record lookup `id`, or the output `domain`: fail.
- **R015-24.** `source` without `key`, or `key` without `source`: fail.
- **R015-25.** `order_by` without `keep`, or `keep` without `order_by`: fail.
- **R015-26.** `source` and `key` lists of different lengths: fail under R007,
  which owns that pairing.
- **R015-27.** No applicable key when neither `source` nor `key` is declared:
  fail under R003.
- **R015-28.** More than one surviving record on a lookup with no `order_by`:
  fail, as an unhandled multiple match under R003.
- **R015-29.** A `between` missing `lower` or `upper`, or naming a column the
  lookup's dataset does not have: fail.
- **R015-30.** A `between.value`, `lower`, and `upper` that are not mutually
  comparable: fail before data is read and report their runtime types.
- **R015-31.** A variable qualified by a record lookup `id` naming a column
  its dataset does not have: fail under R002.
- **R015-32.** A missing declared `source` or `between.value` where
  `incomplete` resolves to `fail`: fail, reporting the record lookup and
  value that is missing.
- **R015-33.** An unmatched left row where `unmatched` resolves to `fail`:
  fail, reporting the record lookup and the offending keys.
