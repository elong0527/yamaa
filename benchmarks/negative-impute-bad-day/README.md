# Reject Nonexistent Day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-bad-day.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete the analysis end date (`AENDT`) for each
adverse event (AE), rejecting any completion that lands on a day
the month never has.

**Input:** adverse event records carrying collected end text
(`AEENDTC`) and reported term (`AETERM`).

**Variables:**

- `AENDT` would be the analysis end date completed from `AEENDTC`:
  a year alone becomes 31 December, a year and month becomes the
  31st of that month, and a complete date is kept as collected. A
  year and month whose month has fewer than 31 days would complete
  to a day that month never has, which is not a calendar date, so
  the run fails and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide which day an end date collected without one should take. To keep the
latest date the collected text allows, name the last day of the month rather
than a number:

```yaml
date_impute:
  source: AE.AEENDTC
  month: 12
  day: last
```

A year and month such as `2023-04` then completes to `2023-04-30` instead of
the impossible `2023-04-31`, `2023-07` still completes to `2023-07-31`, and a
February takes the 28th or 29th according to the year. A fixed day from 1
through 28 is valid in every month too, but it is a different imputation
policy, not a repair of this one.
