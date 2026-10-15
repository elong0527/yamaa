# Reject rounding a coded value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-round-half-away-from-zero-non-numeric-source.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `RNDVAL` by rounding a value to one decimal place
with ties half away from zero.

**Input:** collected demographics carrying coded sex (`SEX`).

**Variables:**

- `RNDVAL` would be the coded sex (`SEX`) rounded to one decimal
  place.

Rounding a coded value has no meaning, so the run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Round a numeric variable rather than the coded sex. The demographics
input carries none here, so add one and declare its type: a field read
without a declared type is text, and rounding it is rejected the same
way.

```yaml
input:
  DM: {path: input/dm.csv, types: {AVAL: float}}

# in the RNDVAL column
round_half_away_from_zero:
  source: DM.AVAL
  digits: 1
```
