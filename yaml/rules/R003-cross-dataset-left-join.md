---
id: R003
title: Cross-Dataset Left Join
status: normative
applies_to: [derivation.source]
depends_on: [R002, R005]
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
performs an automatic left join during column derivation.

The implementation must:

1. Apply declared source filtering, selection, or aggregation to the right side.
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

## Example

ADLB keys are `[STUDYID, USUBJID, PARAMCD, ADT, ASEQ]`. Because ADSL contains
`STUDYID` and `USUBJID`, `ADSL.TRTSDT` joins by those two applicable keys.

## Errors

- No applicable keys: fail.
- An applicable left key is unavailable: fail.
- Multiple right-side matches: fail.
- No right-side match: return missing.
