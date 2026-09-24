# Reject Missing Month

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-missing-month.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a month and a day wherever the collected text does not already
carry them.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC` to the 15th of the supplied month, with `15` as the
  supplied day. Year precision is the minimum accepted, so a month
  is required wherever the policy can use it: omitting it is
  rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Write the month the policy can use. Because this benchmark describes
mid-month imputation, June is the consistent correction:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 6
  day: 15
  minimum_source_precision: year
```

A year-and-month value such as `2023-06` then becomes `2023-06-15`, and a
year-only value such as `2023` becomes `2023-06-15`.
