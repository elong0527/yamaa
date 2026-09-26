# Reject a Day Count from Text

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-epoch-day-non-date.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** count the exposure start (`EXSTDY`) in days since
1970-01-01, for exposure records with sequence number 1.

**Input:** exposure records, each identified by study, subject, and
sequence, carrying the start date as collected text (`EXSTDT`).

**Variables:**

- `EXSTDY` would hold the number of days from 1970-01-01 to the
  exposure start, negative for an earlier date. Only a date can be
  counted, and the start is still text that nothing has read as a
  date, so the run is rejected before any data is read and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

Read the collected start as a date before counting days. The simplest
correction declares the field's type on the input, so the text becomes a date
as it is read and a start that is not a complete date is rejected there:

```yaml
input:
  EX: {path: input/ex.csv, types: {EXSEQ: int, EXSTDT: date}}
```

If a start that is not a complete date should leave the day count empty
instead, bind the text to an internal `date` column that answers the failed
conversion, and count from that column:

```yaml
- name: ASTDT
  type: date
  derivation:
    value:
      source: EX.EXSTDT
    missing: null

- name: EXSTDY
  type: int
  derivation:
    to_epoch_day:
      source: ASTDT
```

`to_epoch_day` takes a variable as its `source`, so a `to_date` nested inside
it is itself rejected.
