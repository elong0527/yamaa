---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [expression.source, expression.aggregate]

---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without repeating join keys in
each specification.

## Boundaries

This rule owns the implicit join a qualified cross-dataset source performs, and
the right-side reduction that precedes it. It does not own `mapping_from`,
whose keys are declared rather than derived from output `keys`, or any window
or row-construction use of `filter`; R007 owns both. A named record lookup
reaching one record for several columns at once is R015, which performs this
join and adds no other way of reaching a right side.

## Terminology

**R003-1.** Constructed output rows are the left side.

**R003-2.** The dataset named by a qualified source is the right side.

**R003-3.** Applicable keys are output `keys` whose names also exist on the
right side.

## Rule

**R003-4.** A qualified source referring to a dataset other than the
current row driver performs an automatic left join during column
derivation. The implementation must take each of the following steps:

1. **R003-5.** Apply any right-side reduction described below.
2. **R003-6.** Select applicable keys in output `keys` order.
3. **R003-7.** Require at least one applicable key.
4. **R003-8.** Require right-side uniqueness on the applicable keys.
5. **R003-9.** Match equality on every applicable key.
6. **R003-10.** Copy the referenced value to matched left rows.
7. **R003-11.** Produce missing when a left row has no match.

**R003-12.** The join is many-to-one and preserves left row count and
order.

**R003-13.** Right records with a missing applicable key cannot match.
Key names must match exactly. String key equality is R019's.

**R003-13a.** The two sides of an applicable key must also carry mutually
comparable types under R007-31, and the match converts no operand. R014-4
gives an undeclared field of a typeless container the type `str`, so an
output key of another type meets such a field nowhere: every row would
receive missing and a complete right side would be reported as an absent
record. A key typed on one side and left to R014's default on the other is
therefore a declaration to repair rather than a comparison to widen. This is
what R007-19 already says between an operation's inputs and R015-11 between
a range's operands; the join is not the one place that converts.

## Declared-key lookup

**R003-14.** `mapping_from` is not this join. Both are equality left
joins that add one column, so they differ only in where the keys come
from.

**R003-15.** This rule's join derives keys from output `keys` that also
exist on the right side. `mapping_from` declares its own pairs of
source variable and right-side column and never consults output `keys`,
so it reaches a right side that is keyed on something else, or that is
not unique on the applicable keys. R007 defines its semantics.

## Right-side reduction

**R003-16.** During column derivation, an aggregate expression whose
identifiers are qualified to a declared dataset reads that relation as
its right side.

**R003-17.** For each current row, applicable keys select a right-side
partition, R013 reduces its eligible records to one value, and that
value is joined back without changing row count.

**R003-18.** The qualifier may equal the current row driver: a scalar
source then reads the current driver record, while an aggregate reads
the driver relation. R007 registers the expression and R013 defines
what it computes.

**R003-19.** Its optional `filter` selects which right-side records
enter that reduction:

```yaml
aggregate:
  expr: "MIN(EX.EXSTDTC)"
  filter: "EX.EXDOSE > 0"
```

**R003-20.** A reduction may declare a `group_by` coarser than the
applicable keys. The join then matches on those columns instead; R013
requires those columns to be output keys.

**R003-21.** A structured `source` declaring `multiple_matches` may also
declare `filter`. It selects which right-side records are eligible
before ordering, using the same evaluation as the aggregate form:

```yaml
source:
  variable: EX.EXTRT
  multiple_matches:
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
    filter: "EX.APERIOD = 1"
```

**R003-22.** In both places `filter` is a predicate over right-side
records only.

**R003-23.** A left row whose right side is empty after filtering has no
match and receives missing, exactly as if no record had existed.

**R003-24.** A qualified aggregate may also be narrowed by the current
left row.

**R003-25.** Its `between` declaration names one `value` the current row
reads and at least one `lower` or `upper` column on the right.

**R003-26.** A record is eligible when every declared comparison holds:
`lower <= value` and `value <= upper`. Every stated endpoint is
inclusive.

**R003-27.** A missing current-row value leaves the aggregate's right
side empty for that row and the result is missing; it never silently
removes the narrowing.

**R003-28.** A right-side record missing a declared bound is ineligible.
R013 defines the complete aggregate contract.

**R003-29.** This is not `row.filter`. R001 makes an ungrouped row
filter select driver records before row derivation and a grouped row
filter select completed candidate groups; neither is a right-side
reduction filter.

## Multiple matches

**R003-30.** By default, multiple right-side matches fail.

**R003-31.** A structured source may declare `multiple_matches` as the
local, explicit relaxation defined by R008. Its `order_by` uses the
order terms defined by R007, so a right-side selection declares
direction and null placement the same way a window does.

**R003-32.** An aggregate does not declare `multiple_matches`.

## Rationale

Join keys come from output `keys` so that specifications do not repeat
them for each cross-dataset reference, while `mapping_from` declares
its own pairs for right sides keyed on something else. Reduction yields
at most one record per grouping key, so an aggregate cannot encounter
multiple matches. A reduction `group_by` coarser than the applicable
keys changes what the join matches on, which is why R013 requires those
columns to be output keys.

Requiring the two sides of an applicable key to be comparable answers the
same hazard the inference itself has: a join that is well formed, holds
uniqueness, and still matches nothing reports a complete right side as an
absent record. Refusing the match makes the missing `types` declaration
visible, where converting an operand would hide it and leave every runtime
free to convert differently.

## Errors

**R003-33.** No applicable keys: fail.

**R003-34.** An applicable left key is unavailable: fail.

**R003-34a.** An applicable key whose two sides are not mutually comparable:
fail before any record is read, reporting the key, the dataset, and the type
each side declares. The condition is the incompatible-input error R007-38
owns, because the defect is a pair of operation inputs that cannot be
compared rather than anything specific to this join.

**R003-35.** Multiple matches after reduction: fail unless locally
handled.

**R003-36.** No right-side match: return missing.

**R003-37.** An aggregate `between` with neither bound, a bound outside
the right-side relation, or operands whose types are not comparable:
fail under R013.

## Review

**R003-38.** Validation reports the inferred applicable keys for every
qualified source, so a reviewer sees which same-named columns the join
matches on, the type each side declares for them, and the coarser grain a
reduction declared in their place.
