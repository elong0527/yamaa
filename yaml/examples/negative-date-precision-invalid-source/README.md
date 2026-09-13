# Reject a start-date completeness flag read from text that is not a date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-precision-invalid-source.html)

**Goal:** one analysis record for each collected event, carrying
the start-date completeness flag `ASTDTF`.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDTF` would contain the start-date completeness read from
  `AESTDTC`: blank when the full date was collected and when the
  source was never collected, and `D` when only the year and month
  were collected. Text that is neither a date nor the beginning of
  one has no completeness to report, so the run stops when it
  tries to read precision from it and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Correct the source text when a date can be recovered. If invalid date text
is intentionally represented by a missing precision flag, declare that
separately from the existing missing-source behavior:

```yaml
date_precision:
  source: AE.AESTDTC
  missing: null
  invalid: null
```

The `invalid` outcome covers `ONGOING`; `missing` covers a source value that
was not collected.
