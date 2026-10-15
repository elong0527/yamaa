# Reject Partial Date as a Datetime Source

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-datetime-impute-bad-source.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the first treatment date/time (`TRTSDTM`) from collected
exposure start text.

**Input:** exposure records carrying a collected start date or date/time.

**Variables:**

- `TRTSDTM`: the collected exposure start date/time, or the first second of a
  completely collected date when only its time is absent. A start collected to
  the year and month only names no day, and supplying a time cannot supply
  one, so the run stops rather than inventing both a day and a time.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Recover the exposure day upstream when it was collected. If the analysis plan
also defines a date-imputation policy, complete the date in a separate named
column before applying `datetime_impute`; do not silently combine two policies.
