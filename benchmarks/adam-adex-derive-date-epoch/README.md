# Derive Date Epoch

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adex-derive-date-epoch.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the aggregate `derive` step for #705: convert
dates to integer epoch days via `to_epoch_day`, then use the result
in an aggregation.

**Input:** exposure records with sequence numbers (`EXSEQ`) and
start dates (`EXSTDT`).

**Variables:**

- `EXSEQ`: the exposure sequence number, carried through.
- `EXSTDY`: the study day of exposure start, as integer days since
  1970-01-01.

**Mechanism:** the `derive` step binds `EPOCHDAY` per record using
the `to_epoch_day` expression, which returns a signed integer
(negative before the epoch, zero on 1970-01-01). The binding's
declared `int` type confirms the integer type. The reducer then
selects `ONLY(EPOCHDAY)`.

**Standard:** ADaM | **Domain:** ADEX
