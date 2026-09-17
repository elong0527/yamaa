---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [expression.source, expression.aggregate]

---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without repeating join keys.

## Boundaries

This rule owns the implicit join from a qualified cross-dataset source and
the source's right-side reduction. R007 owns `mapping_from` and the window
and row-construction `filter` uses. `mapping_from` keys are declared, not
derived from output `keys`. R015 owns a named record lookup that reaches
one record for several columns. A named record lookup performs its own join
and has no other way to reach a right side.

## Terminology

**R003-1.** Constructed output rows are the left side.

**R003-2.** The dataset named by a qualified source is the right side.

**R003-3.** Applicable keys are output `keys` whose names also exist on the
right side.

## Rule

**R003-4.** A qualified source referring to a dataset other than the
current row template's input dataset performs an automatic left
join during column derivation. The implementation must take these steps:

1. **R003-5.** Apply any right-side reduction described below.
2. **R003-6.** Select applicable keys in output `keys` order.
3. **R003-7.** Require at least one applicable key.
4. **R003-8.** Require right-side uniqueness on the applicable keys.
5. **R003-9.** Match equality on every applicable key.
6. **R003-10.** Copy the referenced value to matched left rows.
7. **R003-11.** Produce missing when a left row has no match.

**R003-12.** The join is many-to-one and preserves left row count and
order.

**R003-13.** Right records missing an applicable key cannot match. Key names
must match exactly. R019 defines string key equality.

**R003-13a.** The two sides of an applicable key must have mutually
comparable types under R007-31. The match converts no operand. R014-4 gives
an undeclared field of a typeless container the type `str`. An output key of
another type matches no such field. Without mutually comparable types, every
row would receive missing. A complete right side would be reported as an
absent record. A key typed on one side and given R014's default on the other
needs repair, not a wider comparison. R007-19 sets the same requirement for
operation inputs. R015-11 sets the same requirement for range operands.

## Declared-key lookup

**R003-14.** `mapping_from` is not this join. Both are equality left joins
adding one column; they differ only in where the keys come from.

**R003-15.** This rule's join derives keys from output `keys` that also
exist on the right side. `mapping_from` declares its pairs of source
variable and right-side column without consulting output `keys`, so
`mapping_from` reaches a right side keyed on something else or not unique
on the applicable keys. R007 defines the `mapping_from` semantics.

## Right-side reduction

**R003-16.** During column derivation, an aggregate expression whose
identifiers are qualified to a declared dataset reads the dataset as
the aggregate's right side.

**R003-17.** For each current row, applicable keys select a right-side
partition. R013 reduces eligible records in that partition to one value.
The reduced value joins back without changing row count.

**R003-18.** The qualifier may equal the input dataset of the current row
template. A scalar source then reads the current input record. An
aggregate then reads that dataset. R007 registers the expression. R013
defines the computation.

**R003-18a.** A specification without `rows` reads the input dataset the
same way, over the records its key combination was derived from, which
R001-12 builds. In that case the row has no single input record, so the
source reads one value across those records: R001-44 fails a column that
finds two, and `multiple_matches` is what keeps one of them instead.

**R003-19.** The aggregate's optional `filter` selects which right-side
records enter that reduction:

```yaml
aggregate:
  expr: "MIN(EX.EXSTDTC)"
  filter: "EX.EXDOSE > 0"
```

**R003-20.** A reduction may declare a `group_by` coarser than the
applicable keys. The join then matches on `group_by` instead. R013
requires the `group_by` columns to be output keys.

**R003-21.** A structured `source` may declare `filter`. It selects which
right-side records the source may read, before the source reaches one,
using the same evaluation as the aggregate form:

```yaml
source:
  variable: ODM.Value
  filter: "ODM.ItemOID = 'IT.DM.SEX'"
```

**R003-21a.** A source declaring `multiple_matches` declares `filter`
beside it, not inside it. The filter states which records are eligible and
`multiple_matches` then orders those and keeps one:

```yaml
source:
  variable: EX.EXTRT
  filter: "EX.APERIOD = 1"
  multiple_matches:
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
```

**R003-21b.** Every operation naming a source accepts the filtered form.
Outside the `source` expression it carries `variable` and `filter` alone:
the binding handlers stay on `source`, and an operation that needs
a handler composes through a named column under R002-13.

**R003-22.** In every place `filter` is a predicate over right-side
records only.

**R003-23.** A left row whose right side is empty after filtering has no
match and receives missing, as if no record existed.

**R003-24.** A qualified aggregate may also be narrowed by the current
left row.

**R003-25.** The aggregate's `between` declaration names one `value` the
current row reads and at least one `lower` or `upper` column on the right.

**R003-26.** A record is eligible when every declared comparison holds.
The comparisons are `lower <= value` and `value <= upper`. Every stated
endpoint is inclusive.

**R003-27.** A missing current-row value empties the aggregate's right
side for that row. The result is missing. The missing value never
silently removes the narrowing.

**R003-28.** A right-side record missing a declared bound is ineligible.
R013 defines the aggregate contract.

**R003-29.** A reduction `filter` is not `row.filter`. R001 makes an
ungrouped row filter select input records before row derivation and a
grouped row filter select completed candidate groups. Neither row filter
is a right-side reduction filter.

## Multiple matches

**R003-30.** By default, multiple right-side matches fail.

**R003-31.** A structured source may declare `multiple_matches` as the
local, explicit relaxation defined by R008. The `order_by` uses the
order terms defined by R007. A right-side selection therefore declares
direction and null placement as a window does.

**R003-32.** An aggregate does not declare `multiple_matches`.

## Rationale

Join keys come from output `keys`. Specifications do not repeat keys for each
cross-dataset reference. A reduction yields at most one record per
grouping key, so an aggregate never meets multiple matches. A reduction
`group_by` coarser than applicable keys changes the join keys. R013 requires
`group_by` columns to be output keys.

Comparable key types prevent an empty but otherwise valid join from reporting
a complete right side as absent. Refusing a comparison makes a missing `types`
declaration visible. Converting an operand would hide the missing declaration
and let each runtime choose a conversion.

## Errors

**R003-33.** No applicable keys: fail.

**R003-34.** An applicable left key is unavailable: fail.

**R003-34a.** An applicable key whose two sides are not mutually comparable:
fail before any record is read, reporting the key, the dataset, and the type
each side declares. The condition is the incompatible-input error R007-38
owns, because the defect is a pair of operation inputs that cannot be
compared, not anything specific to this join.

**R003-35.** Multiple matches after reduction: fail unless locally
handled.

**R003-36.** No right-side match: return missing.

**R003-37.** An aggregate `between` with neither bound, a bound outside
the right-side relation, or operands whose types are not comparable:
fail under R013.

**R003-38.** A `filter` on a source with no records to select among: fail
as a prohibited construct. An unqualified source reads one completed
output column, a grouped row template's source reads one group key
(R001-7), and a record lookup has already chosen its record (R015).

## Review

**R003-39.** Validation reports the inferred applicable keys for every
qualified source. A reviewer sees the same-named columns the join matches.
The report shows the type each side declares and any coarser reduction grain
in place of the keys.
