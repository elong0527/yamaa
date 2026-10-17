# Reject Missing Month

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a month and a day wherever the collected text does not already
carry them.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`), at year,
year-and-month, and full-date precision.

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC`: the 15th of the month for a year-and-month start, the
  15th of the stated month for a year-only start, and the collected
  date unchanged when it is already complete. The policy accepts
  year-only starts, so it needs a month to give them, but the
  specification names none. The run is rejected before any data is
  read and no artifact is accepted.

## How to fix

Write the month the policy can use. Because this benchmark describes
mid-month imputation, June is the consistent correction:

```yaml
impute:
  type: date
  source: AE.AESTDTC
  month: 6
  day: 15
  minimum_source_precision: year
```

A year-and-month value such as `2023-06` then becomes `2023-06-15`, and a
year-only value such as `2023` becomes `2023-06-15`.
