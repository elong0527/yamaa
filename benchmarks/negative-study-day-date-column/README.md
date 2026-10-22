# Reject a Study Day Stored as a Date

**Goal:** one row per adverse event (AE) with its analysis start date
(`ASTDT`) and the study day of that start (`ASTDY`).

**Input:** collected adverse events with the onset date (`AESTDTC`) and the
subject's first treatment date (`TRTSDT`).

**Variables:**

- `ASTDT` is the onset date as collected.
- `TRTSDT` is the date of the first exposure to treatment.
- `ASTDY` would be the study day of the onset: the first treatment date is
  day 1 and the day before it is day -1. A study day is a count of days, and
  the column meant to hold it holds calendar dates, so the run is rejected
  before any data is read and no artifact is accepted.

## How to fix

Store the study day as an integer, the type every study day has:

```yaml
- name: ASTDY
  type: int
  label: Analysis Start Relative Day
  derivation:
    study_day:
      source: ASTDT
      reference: TRTSDT
```

A specification that needs the calendar date of the onset already has it in
`ASTDT`.
