# Reject Unknown Day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-day-rule.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the analysis start date (`ASTDT`) for each
collected adverse event (AE), completing a start recorded without
a day.

**Input:** collected adverse events carrying the reported start
(`AESTDTC`), which is sometimes a year and month with no day.

**Variables:**

- `ASTDT` would contain the collected start, kept as collected
  when it is a complete date and otherwise completed with day
  `mid` (and month `6` for a year alone).

The day value `mid` is neither a day number nor one of the two
positions every month has, `first` and `last`, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Decide where in the month the analysis date should fall, then name a position
a calendar fixes or name the day itself. To place a start recorded as a year
and month on the first of that month:

```yaml
date_impute:
  source: AE.AESTDTC
  month: 6
  day: first
```

A value such as `2023-06` then becomes `2023-06-01`. Writing `day: last`
instead gives `2023-06-30`, and writing `day: 15` names the fifteenth of the
collected month directly.
