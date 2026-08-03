---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [derivation.source, derivation.filter, derivation.operations]
depends_on: [R002, R005, R007, R008]
---

# Cross-dataset left join

## Intent

Enrich constructed rows from another dataset without explicit join statements
in each specification.

## Terminology

- Constructed output rows are the left side.
- The dataset named by a qualified source reference is the right side.
- Applicable keys are output `keys` whose names also exist in the right-side
  dataset.

## Rule

A qualified reference to a dataset other than the current row-driving dataset
performs an automatic left join during column derivation. This applies both to
the derivation's `source` and to `{source: VARIABLE}` expressions nested in
operation arguments.

The implementation must:

1. Apply declared right-side reduction as defined below.
2. Select applicable keys in output `keys` order.
3. Require at least one applicable key.
4. Require right-side uniqueness on the applicable keys.
5. Match using equality on every applicable key.
6. Copy the referenced value to matched left rows.
7. Produce missing when a left row has no match.

The operation is many-to-one and must preserve left row count and order.
Right-side records with a missing applicable key cannot match. Key names must
match exactly; differently named keys must be aligned before derivation.

A reference to the current row-driving dataset reads the current source record
directly and does not perform a join.

## Dictionary lookups are not joins

This rule matches on applicable keys, meaning output `keys` that also exist in
the right side. A coding dictionary is matched on the coded value instead, which
is never an output key, so this rule cannot express it.

That case is the `mapping_from` operation in R007. It names its dataset, match
column, and return column explicitly, and matches the pipeline value rather than
the output keys. Both mechanisms are many-to-one, require right-side uniqueness
on whatever they match, and preserve left row count and order.

## Right-side reduction

A right side that holds several records per key must be reduced to one record
before the join. Two declarations perform that reduction, and both apply to the
right side only.

`derivation.filter` is a predicate that removes right-side records before
aggregation. It is valid only in a derivation that performs a join.

It is not `row.filter`. Both fields are named `filter` and both take exactly one
`sql` predicate, but `row.filter` selects records from the row-driving dataset
during row construction under R004, while `derivation.filter` narrows the right
side of a join during column derivation.

An `aggregate` operation placed first in the pipeline reduces the remaining
right-side records, partitioned by the applicable keys, as defined by R007.
Later operations in the pipeline run after the join, on the constructed output
rows.

```yaml
TRTSDT:
  source: EX.EXSTDTC
  filter: "EX.EXDOSE > 0"
  operations:
    - min: {}
```

The reduction is what makes the join legal. Without it the same reference fails
on right-side uniqueness. `group_by` plays no part here: it partitions output
rows, and by the time it applies the join has already been evaluated.

## Example

ADLB keys are `[STUDYID, USUBJID, PARAMCD, ADT, ASEQ]`. Because ADSL contains
`STUDYID` and `USUBJID`, `ADSL.TRTSDT` joins by those two applicable keys.

## Errors

- No applicable keys: fail.
- An applicable left key is unavailable: fail.
- Multiple right-side matches after reduction: fail, unless the derivation
  declares a `multiple_matches` exception, which R008 permits to relax this
  requirement.
- No right-side match: return missing.
- `derivation.filter` in a derivation that performs no join: fail.
