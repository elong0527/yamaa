# Reject a first-known-alive date taken from a day number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-least-incomparable-sources.html)

**Goal:** record `DTHDT` and `LSTVSDY` and keep the earlier of the
two in `FSTALVDT` as the first known alive date.

**Input:** collected demographics carrying a death date-time
(`DTHDTC`) and the last-visit study day (`LSTVSDY`).

**Variables:**

- `DTHDT` would be the date of death taken from `DTHDTC`, missing
  for subjects still followed.
- `LSTVSDY` would be the study day of the last visit, counting
  days, not dates.
- `FSTALVDT` is intended as the earlier of the two dates as the
  first known alive date.

A calendar date and a day count have no common order, so the run
is rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which date the study reports, then state it in dates. When the last
contact is only known as a study day, convert it against the reference start
date first so both sides are dates:

```yaml
- name: FSTALVDT
  type: date
  derivation:
    least:
      sources: [DTHDT, LSTCONDT]
```

When only the day count exists, keep the count as a count instead of forcing
it into a date column.
