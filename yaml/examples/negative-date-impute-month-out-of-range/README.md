# Reject a start date completed with an impossible month

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-impute-month-out-of-range.html)

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a month and a day only where the collected text already carries a
month.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC` to the earliest date the collected text still allows,
  with `15` as the supplied month and `1` as the supplied day. A
  year and month without a day would be completed, a year alone
  would be left missing rather than given both a month and a day,
  and a fully collected date would be used as collected. The value
  `15` is not a calendar month, so the run is rejected before any
  data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use a calendar month from 1 through 12. Because this example describes earliest
imputation, January is the consistent correction:

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
`minimum_source_precision` as well would let `2023` become `2023-01-01`.
