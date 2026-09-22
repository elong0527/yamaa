# Reject a Laboratory Record with an Unconverted Unit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-sdtm-lb-unit-conversion.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** standardize reported laboratory results to one unit per test --
glucose to `mmol/L`, hemoglobin to `g/dL` -- while keeping each reported
result, reported unit, and reported reference range untouched.

**Input:** one row per laboratory record carrying the test code
(`LBTESTCD`), the reported result (`LBORRES`), the reported unit
(`LBORRESU`), and the reported reference limits (`LBORNRLO`,
`LBORNRHI`); plus a conversion table carrying test code, reported unit,
and the factor that multiplies the reported result into the standard
unit. Two subjects convert cleanly -- glucose in `mg/dL`, hemoglobin in
`g/L`, and records already in the standard unit -- while a third
subject's hemoglobin is reported in `mmol/L`, which the table does not
cover.

**Variables:**

- `LBSTRESN` would be the reported result times the conversion table
  factor, missing when the table has no factor for the reported unit.
- `LBSTRESC` would be the standardized numeric result written as text,
  so it always agrees with `LBSTRESC`'s numeric sibling.
- `LBSTRESU` would be the standard unit for the test: `mmol/L` for
  glucose, `g/dL` for hemoglobin.
- `LBORRES`, `LBORRESU`, `LBORNRLO`, and `LBORNRHI` keep the reported
  result, unit, and limits exactly as collected, but this run is
  rejected so no dataset is accepted.
- `LBDTC` carries the laboratory collection date from the input.

The hemoglobin record reported in `mmol/L` finds no factor in the
conversion table, so its standardized result is missing while the
record keeps its reported values. The completeness check requires the
standardized numeric result, its text form, and its standard unit to be
present together on every record, so the run is rejected and no
artifact is accepted. Passing the reported values through as if they
were standardized would silently misstate the unit.

**Note:** a reported unit the table does not cover is different from a
record that needs no conversion. Records already in the standard unit
carry a factor of one and pass the check; only an uncovered unit fails.

**Standard:** SDTM | **Domain:** LB

## How to fix

Cover the unit in the conversion table -- add the missing test-and-unit
row with its factor -- and rerun; the completeness check then passes on
every record. The conversion lookup already names its missing-unit
answer: a reported unit absent from the table yields a missing factor,
and the check rejects the run instead of passing the original through.
Do not default the factor to one for uncovered units, and do not drop
the completeness check -- either would turn a data problem the study
must resolve into a quietly wrong dataset.
