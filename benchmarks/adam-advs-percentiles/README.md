# Growth Percentiles

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-percentiles.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add a growth-percentile record for each collected body
measurement, holding the percentile in `AVAL`. A body mass index
(`BMI`) becomes `BMIPCTL`, named `BMI-for-Age Percentile`, and a
weight (`WEIGHT`) becomes `WGTPCTL`, named `Weight-for-Age
Percentile`.

**Input:** collected measurements carrying the measurement code,
the collected result (`AVAL`), sex (`SEX`), and age in days
(`AGE`), alongside a sex-and-age growth reference carrying the
coefficients `L`, `M`, and `S` for each measurement code, sex,
and age in days.

**Variables:**

- `AVAL`: the growth percentile, equal to 100 times the standard
  normal cumulative probability of the z-score ((collected `AVAL`
  / `M`) raised to the power `L`, minus 1) divided by (`L` times
  `S`), where `L`, `M`, and `S` are the reference coefficients
  matched on measurement code, `SEX`, and `AGE`. A measurement
  with no matching reference row leaves `AVAL` missing.

**Standard:** ADaM | **Domain:** ADVS
