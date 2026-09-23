# Reject Impossible Month

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-bad-month.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a day only where the collected text already carries a month.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC` to the earliest date the collected text still allows:
  a year and month without a day would take day `1`, a year alone
  would be left missing rather than given both a month and a day,
  and a fully collected date would be used as collected. The
  stated month `15` is therefore never used, but it is still
  checked, and it is not a calendar month, so the run is rejected
  before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use a calendar month from 1 through 12. Under this policy the month is never
used, because a year alone is left missing and a year and month keep their
own, but it must still be a real month. January matches the earliest-date
intent should the policy later admit a year alone:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 1
  day: 1
  minimum_source_precision: month
```

A year-and-month value such as `2023-06` then becomes `2023-06-01`. A
year-only value such as `2023` is still left missing, because the declared
minimum forbids supplying both a month and a day. Dropping
`minimum_source_precision` as well would let `2023` become `2023-01-01`, the
only case in which the month is used.
