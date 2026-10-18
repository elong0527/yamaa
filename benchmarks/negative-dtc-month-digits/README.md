# Reject a Collected Month Written as One Digit

**Goal:** write each adverse event's collected start (`AESTDTC`) as ISO
8601 text from the start year, month and day collected on the form.

**Input:** collected adverse event records, each identified by study,
subject and sequence, carrying the reported term and the start year,
month and day as separate text fields; a part that was not collected is
empty.

**Variables:**

- `AESTDTC` would be the collected start at the precision collected: a
  full date (`2026-03-15`), a year and month, or a year alone, stopping
  before the first part that was not collected. A month written as one
  digit, such as `3`, is not an ISO 8601 month, so the run is rejected
  and no artifact is accepted.

## How to fix

Correct the collected data first: an ISO 8601 month is two digits, so
the month the site recorded as `3` is `03` at its source. When the
extract delivers the parts as numbers, read them as integers on the input
instead; an integer part is written with its leading zero:

```yaml
input:
  AE_RAW: {path: input/ae_raw.csv, types: {AESEQ: int, AESTMO: int}}
```

If a malformed part should leave the start empty rather than stop the
run, declare that outcome explicitly with `invalid: {literal: null}` on
the `dtc` operation, and review the records it empties.
