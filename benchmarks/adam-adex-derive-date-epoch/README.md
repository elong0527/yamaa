# Derive Date Epoch

[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the aggregate `derive` step for #705: convert
dates to integer epoch days via `to_epoch_day`, then use the result
in an aggregation.

**Input:** exposure records with start dates (`EXSTDT`).

**Variables:**

- `EXSTDY`: the study day of exposure start, as integer days since
  1970-01-01.

**Mechanism:** the `derive` step binds `EPOCHDAY` per record using
the `to_epoch_day` expression, which returns a signed integer
(negative before the epoch, zero on 1970-01-01). The binding's
`type: int` confirms the integer type. The reducer then selects
`ONLY(EPOCHDAY)`.
