# Reject Incomplete Date

**Goal:** carry the collected adverse event (AE) start text
(`AESTDTC`) toward `ASTDT`.

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

## How to fix

Declare how a partial date is completed. For example, to use the earliest date
the collected text permits:

```yaml
derivation:
  impute:
    type: date
    source: AE.AESTDTC
    month: 1
    day: 1
```

A complete source date is retained; `2023-06` becomes `2023-06-01`.
