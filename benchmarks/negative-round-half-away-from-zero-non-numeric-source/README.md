# Reject rounding a coded value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-round-half-away-from-zero-non-numeric-source.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** build `RNDVAL` by rounding a value to one decimal place
with ties half away from zero.

**Input:** collected demographics carrying coded sex (`SEX`).

**Variables:**

- `RNDVAL` would be the coded sex rounded to one decimal place,
  which comes from `SEX` in the collected demographics, but no row
  is produced.

Rounding a coded value has no meaning, so the run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Round a numeric variable rather than the coded sex:

```yaml
round_half_away_from_zero:
  source: DM.AVAL
  digits: 1
```
