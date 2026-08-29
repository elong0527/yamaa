---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [expression.source, expression.aggregate]
depends_on: [R002, R004, R005, R007, R008, R013]
---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without repeating join keys in
each specification.

## Boundaries

This rule owns the implicit join a qualified cross-dataset source performs, and
the right-side reduction that precedes it. It does not own `mapping_from`,
whose keys are declared rather than derived from output `keys`, or any window
or row-construction use of `filter`; R007 owns both.

## Terminology

- Constructed output rows are the left side.
- The dataset named by a qualified source is the right side.
- Applicable keys are output `keys` whose names also exist on the right side.

## Rule

A qualified source referring to a dataset other than the current row driver
performs an automatic left join during column derivation. The implementation
must:

1. Apply any right-side reduction described below.
2. Select applicable keys in output `keys` order.
3. Require at least one applicable key.
4. Require right-side uniqueness on the applicable keys.
5. Match equality on every applicable key.
6. Copy the referenced value to matched left rows.
7. Produce missing when a left row has no match.

The join is many-to-one and preserves left row count and order. Right records
with a missing applicable key cannot match. Key names must match exactly.

## Declared-key lookup

`mapping_from` is not this join. Both are equality left joins that add one
column, so they differ only in where the keys come from. This rule's join
derives them from output `keys` that also exist on the right side.
`mapping_from` declares its own pairs of source variable and right-side column
and never consults output `keys`, so it reaches a right side that is keyed on
something else, or that is not unique on the applicable keys. R007 defines its
semantics.

## Right-side reduction

An aggregate expression whose identifiers are qualified to another dataset
reduces that right side by applicable keys before joining. R007 registers the
expression and R013 defines what it computes. Its optional `filter` selects
which right-side records enter that reduction:

```yaml
aggregate:
  expr: "MIN(EX.EXSTDTC)"
  filter: "EX.EXDOSE > 0"
```

A reduction may declare a `group_by` coarser than the applicable keys. The
join then matches on those columns instead, which R013 requires to be output
keys for exactly that reason.

A structured `source` declaring `multiple_matches` may also declare `filter`.
It selects which right-side records are eligible before ordering, using the
same evaluation as the aggregate form:

```yaml
source:
  variable: EX.EXTRT
  multiple_matches:
    order_by: [EX.EXSTDTC, EX.EXSEQ]
    keep: first
    filter: "EX.APERIOD = 1"
```

In both places `filter` is a predicate over right-side records only. A left row
whose right side is empty after filtering has no match and receives missing,
exactly as if no record had existed.

This is not `row.filter`, which selects row-driver records during row
construction, before any column is derived.

## Multiple matches

By default, multiple right-side matches fail. A structured source may declare
`multiple_matches` as the local, explicit relaxation defined by R008. Its
`order_by` uses the order terms defined by R007, so a right-side selection
declares direction and null placement the same way a window does.

Reduction already yields at most one right-side record per key it groups on,
so an aggregate cannot encounter multiple matches and does not declare
`multiple_matches`.

## Errors

- No applicable keys: fail.
- An applicable left key is unavailable: fail.
- Multiple matches after reduction: fail unless locally handled.
- No right-side match: return missing.
