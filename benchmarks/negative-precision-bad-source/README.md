# Reject Non-Date Flag

**Goal:** one analysis record for each collected event, carrying
the start-date completeness flag `ASTDTF`.

**Input:** collected event records carrying the reported term
(`AETERM`) and the collected start (`AESTDTC`).

**Variables:**

- `ASTDTF` would contain how much of the start date `AESTDTC`
  carries: `day` for a full date, `month` for a year and month,
  `year` for a year alone, and blank when the start was never
  collected. Text
  that is neither a date nor the beginning of one has no
  completeness to report, so the run stops when it tries to read
  precision from it and no artifact is accepted.

## How to fix

Correct the source text when a date can be recovered. If invalid date text
is intentionally represented by a missing precision flag, declare that
outcome explicitly:

```yaml
precision:
  type: date
  source: AE.AESTDTC
  invalid: {literal: null}
```

The `invalid` outcome covers `UNK`; a source value that was not collected
already gives a missing flag.
