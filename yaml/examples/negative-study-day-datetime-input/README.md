# Reject a study day measured from a moment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-study-day-datetime-input.html)

**Goal:** build one record per subject carrying `RFSTD`, `DTHDTM`,
and `DTHDY`.

**Input:** collected demographics rows carrying the treatment-start
date (`RFSTDT`) and the collected moment of death (`DTHDTC`).

**Variables:**

- `RFSTD` would hold the treatment-start date copied from the
  collected date.
- `DTHDTM` would hold the moment of death, time of day included,
  copied from the collected moment.
- `DTHDY` would hold the study day of death, counting whole days
  from the treatment-start date to the moment; but no row is
  produced.

A study day counts calendar dates, and a moment is not one:
widening the moment into a date would choose silently between two
adjacent days, so no reader may do so. The request is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which calendar day the study reports, then state it as a date. When
only the moment is collected, take its calendar date first so the day count
has whole days to count:

```yaml
- name: DTHDT
  type: date
  derivation:
    to_date:
      source: DTHDTM
```

When the time of day carries meaning, a day count is the wrong result; keep
the moment instead of forcing it into one.
