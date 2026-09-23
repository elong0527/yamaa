# Reject Unreachable Month

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-unreachable-month.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from the collected start (`AESTDTC`), supplying
a day only where the collected text already carries a month.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from
  `AESTDTC` to the 15th of the collected month, with `6` as the
  supplied month and `15` as the supplied day. Month precision is
  the minimum accepted, so no collected start can ever need a
  month: the written `6` is unreachable and the run is rejected
  before any data is read, without checking whether `6` is a
  calendar month. No artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Drop the month: nothing the policy accepts can use it.

```yaml
date_impute:
  source: AE.AESTDTC
  day: 15
  minimum_source_precision: month
```

A year-and-month value such as `2023-06` then becomes `2023-06-15`.
A year-only value such as `2023` is still left missing, because the
declared minimum forbids supplying both a month and a day.
