# Reject counting days between a moment and a date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-diff-datetime-endpoints.html)

**Goal:** build a subject-level record carrying the moment
treatment started in `RFSTDTM` and the calendar date of death in
`DTHDT` and counting whole days between them in `SURVDD`.

**Input:** collected demographics carrying the treatment-start
moment (`RFSTDTC`) and the death date (`DTHDT`).

**Variables:**

- `RFSTDTM` would be the moment treatment started, time of day
  included, taken from `RFSTDTC`.
- `DTHDT` would be the calendar date of death, taken from `DTHDT`.
- `SURVDD` would be the whole-day count from `RFSTDTM` to `DTHDT`.

A day count has no meaning between a moment and a date: the hours
on either side could give two different answers, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which calendar days the study counts, then state them as dates. When
only moments are collected, take each moment's calendar date first so the
count has whole days to count:

```yaml
- name: RFSTD
  type: date
  derivation:
    to_date:
      source: RFSTDTM
```

When the hours matter, the study needs a finer unit than days, which this
vocabulary does not offer; record that gap instead of rounding it away.
