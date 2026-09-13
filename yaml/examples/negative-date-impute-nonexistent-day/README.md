# Reject an end date completed past the end of its month

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-impute-nonexistent-day.html)

**Goal:** complete the analysis end date (`AENDT`) for each
adverse event (AE), rejecting any completion that lands on a day
the month never has.

**Input:** adverse event records carrying collected end text
(`AEENDTC`) and reported term (`AETERM`).

**Variables:**

- `AENDT` would be the analysis end date completed from `AEENDTC`
  with month `12` and day `31`. A year and month whose month has
  fewer than 31 days would complete to a day that month never
  has, which is not a calendar date, so the run fails and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Choose a fixed day that is valid for every source month covered by the rule.
For example, an earliest-date policy can use:

```yaml
date_impute:
  source: AE.AEENDTC
  month: 12
  day: 1
```

A year and month such as `2023-04` would otherwise complete to `2023-04-31`.
If the analysis requires the actual last day of each month, one fixed `day`
cannot express that policy for months of different lengths. Supply complete,
calendar-valid end dates upstream or implement that separately defined rule
through the project's extension point.
