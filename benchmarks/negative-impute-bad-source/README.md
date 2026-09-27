# Reject Non-Date Source

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-impute-bad-source.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete the analysis start date (`ASTDT`) of each
adverse event (AE) from its collected onset text (`AESTDTC`).

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected onset text (`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date completed from the
  onset text: a complete collected date unchanged, a year-and-month
  completed to its first day, a year-only start completed to
  January 1. Text that cannot be read as a date or the beginning
  of one has no stated completion, so the run stops when it reaches
  it and no artifact is accepted.

**Note:** a date that was never collected and text that cannot be
read as a date are different defects, and neither has a stated
outcome here.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Correct `UNKNOWN` upstream when a date can be recovered. If invalid date text
is intentionally treated as no analysis date, declare that outcome locally:

```yaml
derivation:
  date_impute:
    source: AE.AESTDTC
    month: 1
    day: 1
    invalid: null
```

The `invalid` outcome applies to text that is not a calendar date or date
beginning; it is distinct from text that was never collected, which
`missing` answers.
