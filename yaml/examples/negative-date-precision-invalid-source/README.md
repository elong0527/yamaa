# ADaM ADAE: reject a completeness flag read from text that is not a date

This example uses collected adverse events whose start dates are sometimes
incomplete to attempt one record per event:

- `ASTDTF` says how much of the start date was collected, so that a reader can
  tell a recorded day from a supplied one.

One start date was entered as a word rather than as a date or the beginning of
one, and the specification answers only for a date that was never collected.
Reporting the text as fully collected, or as collected to no precision at all,
would each describe a value nobody can read as a date, so the run must fail and
no artifact is accepted.

## How to fix

Correct the source text when a date can be recovered. If invalid date text is
intentionally represented by a missing precision flag, declare that separately
from the existing missing-source behavior:

```yaml
date_precision:
  source: AE.AESTDTC
  missing: null
  invalid: null
```

The `invalid` handler covers `ONGOING`; `missing` covers a source value that
was not collected.
