# Reject a start date completed with an unrecognized day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-impute-unknown-day-rule.html)

**Goal:** build the analysis start date (`ASTDT`) for each
collected adverse event (AE) whose start is recorded without a
day.

**Input:** collected adverse events carrying the reported start
(`AESTDTC`), which is sometimes a year and month with no day.

**Variables:**

- `ASTDT` would contain the collected start completed with month
  `6` and day `mid`.

The day value `mid` is not a position a calendar fixes for every
month. Only `first` and `last` name a day every month has, so the
run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide where in the month the analysis date should fall, then name a position
a calendar fixes or name the day itself. To keep an event as early as the
collected text allows:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 6
  day: first
```

A value such as `2023-06` then becomes `2023-06-01`. Writing `day: last`
instead gives `2023-06-30`, and writing `day: 15` names the fifteenth of the
collected month directly.
