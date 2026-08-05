---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [expression.source, expression.min, expression.max]
depends_on: [R002, R005, R007]
---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without repeating join keys in
each specification.

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

## Dictionary lookup

`mapping_from` is not this join. It matches its source value against an
explicit dictionary column and returns another column. It does not use output
keys.

## Right-side reduction

A structured source can filter right-side records before aggregation. A `min`
or `max` expression whose `value` is that qualified source reduces the right
side by applicable keys before joining:

```yaml
min:
  value:
    source:
      variable: EX.EXSTDTC
      filter: "EX.EXDOSE > 0"
```

`source.filter` is valid only for a cross-dataset source consumed directly by
`min` or `max`. It differs from `row.filter`, which selects row-driver records
during row construction. `group_by` is not used for right-side reduction; it
partitions constructed output rows under R007.

## Multiple matches

By default, multiple right-side matches fail. A structured source may declare
`multiple_matches` as the local, explicit relaxation defined by R008.

## Errors

- No applicable keys: fail.
- An applicable left key is unavailable: fail.
- Multiple matches after reduction: fail unless locally handled.
- No right-side match: return missing.
- `source.filter` outside the reduction context above: fail.
