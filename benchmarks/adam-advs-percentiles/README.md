# Derive growth percentile records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-percentiles.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** turn each collected body measurement (`BMI` or `WEIGHT`)
into one growth-percentile record holding the percentile in `AVAL`;
the collected records are not kept.

**Input:** collected measurements carrying the measurement code
(`PARAMCD`), the collected result (`AVAL`, stored as text), sex
(`SEX`), and age in days (`AGE`); alongside a sex-and-age growth
reference carrying the coefficients `L`, `M`, and `S` for each
measurement code, sex, and age in days.

**Variables:**

- `PARAMCD`: `BMIPCTL` for a collected `BMI` measurement,
  `WGTPCTL` for a collected `WEIGHT` measurement.
- `PARAM`: `BMI-for-Age Percentile` / `Weight-for-Age Percentile`,
  matching `PARAMCD`.
- `AVAL`: the growth percentile, 100 times the standard normal
  cumulative probability of the LMS z-score: the collected result
  divided by `M`, raised to the power `L`, minus 1, all divided by
  `L` times `S`. The `L`, `M`, and `S` are the reference
  coefficients matched on measurement code, sex, and age in days.
  A measurement with no matching reference row, or with no
  collected result, leaves `AVAL` missing.

**Note:** a collected result exactly at the reference median (`M`)
has a z-score of 0 and so a percentile of exactly 50.

**Standard:** ADaM | **Domain:** ADVS
