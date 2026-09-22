# Reject a Laboratory Record with an Unconverted Unit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-sdtm-lb-unit-conversion.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** standardize a reported hemoglobin result to `g/dL` while
keeping the reported result, reported unit, and reported reference
range untouched.

**Input:** one hemoglobin record reported in `mmol/L`, carrying the
test code (`LBTESTCD`), the reported result (`LBORRES`), the reported
unit (`LBORRESU`), and the reported reference limits (`LBORNRLO`,
`LBORNRHI`); plus a conversion table that covers hemoglobin in `g/dL`
only. A reported unit the table does not cover yields a missing
factor.

**Variables:**

- `LBSTRESN` would be the reported result times the conversion table
  factor, missing when the reported unit is not in the table.
- `LBSTRESC` would be the standardized numeric result written as text,
  so it always agrees with `LBSTRESN`.
- `LBSTRESU` would be the standard unit, `g/dL`.
- `LBSTNRLO` and `LBSTNRHI` would be the reported reference limits
  times the same factor, missing with them.
- `LBORRES`, `LBORRESU`, `LBORNRLO`, and `LBORNRHI` keep the reported
  result, unit, and limits exactly as collected, but this run is
  rejected so no dataset is accepted.
- `LBDTC` carries the laboratory collection date from the input.

The record's reported unit (`mmol/L`) is absent from the conversion
table, so no factor is found: the standardized result and the
converted limits are missing while the reported values are kept. The
completeness check requires the standardized numeric result, its text
form, its standard unit, and the converted reference limits to be
present together on every record, so the run is rejected and no
artifact is accepted. Passing the reported values through as if they
were standardized would silently misstate the unit. (A unit the table
does cover would carry its factor and pass the check.)

**Standard:** SDTM | **Domain:** LB

## How to fix

Add the missing unit to the conversion table -- the test-and-unit row
with its factor -- and rerun; the completeness check then passes.
Do not default the factor to one for uncovered units, and do not drop
the completeness check -- either would turn a data problem the study
must resolve into a quietly wrong dataset.
