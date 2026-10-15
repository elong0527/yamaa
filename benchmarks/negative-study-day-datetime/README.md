# Reject Study Datetime

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-study-day-datetime.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per subject carrying `RFSTD`, `DTHDTM`,
and `DTHDY`.

**Input:** collected demographics rows carrying the treatment-start
date (`RFSTDT`) and the collected moment of death (`DTHDTC`).

**Variables:**

- `RFSTD` would hold the treatment-start date copied from the
  collected date.
- `DTHDTM` would hold the moment of death, time of day included,
  copied from the collected moment, and would be blank when no death
  is recorded.
- `DTHDY` would hold the study day of death, with the treatment-start
  date as day 1 and no day zero, and would be blank when no death is
  recorded.

A study day counts calendar dates, and a moment is not one:
widening the moment into a date would choose silently between two
adjacent days, so no reader may do so. The request is rejected
before any data is read, and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which calendar day the study reports, then state it as a date. When
only the moment is collected, take its calendar date first and count the
study day from that date:

```yaml
- name: DTHDT
  type: date
  label: Date of Death
  derivation:
    to_date:
      source: DTHDTM

- name: DTHDY
  type: int
  label: Study Day of Death
  derivation:
    study_day:
      date: DTHDT
      reference: RFSTD
```

When the time of day carries meaning, a day count is the wrong result; keep
the moment instead of forcing it into one.
