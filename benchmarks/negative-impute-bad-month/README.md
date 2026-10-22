# Reject Impossible Month

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a month and a day wherever the collected text does not already
carry them.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC` to the earliest date the collected text still allows,
  with `15` as the supplied month and `1` as the supplied day. A
  year and month without a day would be completed, a year alone
  would be completed from the supplied month and day, and a fully
  collected date would be used as collected. The value
  `15` is not a calendar month, so the run is rejected before any
  data is read and no artifact is accepted.

## How to fix

Use a calendar month from 1 through 12. Because this benchmark describes
earliest imputation, January is the consistent correction:

```yaml
impute:
  type: date
  source: AE.AESTDTC
  month: 1
  day: 1
```

A year-and-month value such as `2023-06` then becomes `2023-06-01`, and a
year-only value such as `2023` becomes `2023-01-01`.
