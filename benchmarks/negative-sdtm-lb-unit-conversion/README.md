# Reject a Laboratory Result Without a Unit Conversion

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-sdtm-lb-unit-conversion.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** convert hemoglobin results and reference limits to `g/dL`,
and reject a result whose reported unit has no conversion factor.

**Input:** hemoglobin results with test code (`LBTESTCD`), reported
units, and reference limits, plus a conversion table keyed by test
code and reported unit.

**Variables:**

- `LBORRES` and `LBORRESU` keep the reported result and unit.
- `LBORNRLO` and `LBORNRHI` keep the reported reference limits.
- `LBSTRESN` is the reported result multiplied by the matching factor;
  it is missing when no factor matches.
- `LBSTRESC` is the standardized numeric result written as text, or
  missing with `LBSTRESN`.
- `LBSTRESU` is the standard unit, `g/dL`.
- `LBSTNRLO` and `LBSTNRHI` are the reported limits multiplied by the
  same factor, or missing when no factor matches.

When a reported unit is absent from the conversion table, the standard
unit remains known but its result and reference limits are missing.
Those standardized values must be present together, so the run is
rejected and no dataset is accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Confirm the appropriate conversion for the uncovered hemoglobin unit,
add its test-and-unit row with the correct factor, and rerun. Do not
assume a factor of one for an uncovered unit.
