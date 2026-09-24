# Reject Incomplete Date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-date-incomplete.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the collected adverse event (AE) start text
(`AESTDTC`) into `ASTDT`.

**Input:** adverse event records, each identified by study,
subject, and sequence, carrying reported term (`AETERM`) and
start text (`AESTDTC`).

**Variables:**

- `ASTDT`: the event start date, which would carry the collected
  start text (`AESTDTC`). A start collected to the month only, such
  as `2023-06`, names no single day, and choosing the first, the
  middle, or the last day would record a day nobody collected. Such
  text cannot be read as a date, so the run fails and no artifact
  is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Declare how a partial date is completed. For example, to use the earliest date
the collected text permits:

```yaml
derivation:
  date_impute:
    source: AE.AESTDTC
    month: 1
    day: 1
```

A complete source date is retained; `2023-06` becomes `2023-06-01`.
