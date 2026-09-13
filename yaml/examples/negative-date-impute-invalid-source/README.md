# Reject a start date completed from text that is not a date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-impute-invalid-source.html)

**Goal:** complete the analysis start date (`ASTDT`) of each
collected adverse event (AE) from its collected onset text.

**Input:** collected adverse event records carrying onset text
(`AESTDTC`).

**Variables:**

- `ASTDT` would be the analysis start date of the event,
  completed to the earliest calendar date the collected onset
  text (`AESTDTC`) still allows, and missing when the text
  carries no date at all.

Onset text entered as a word rather than as a date or the
beginning of one has no stated completion. A date that was never
collected and text that cannot be read as a date are different
defects, so the run stops when it reaches the unreadable text and
no artifact is accepted.

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
beginning; it is distinct from text that was never collected.
