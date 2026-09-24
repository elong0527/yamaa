# Reject Mixed Types

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-least-mixed.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** record `DTHDT` and `LSTVSDY` and keep the earlier of the
two in `FSTALVDT` as the first known alive date.

**Input:** collected demographics carrying a death date-time
(`DTHDTC`) and the last-visit study day (`LSTVSDY`).

**Variables:**

- `DTHDT` would be the date of death taken from `DTHDTC`, missing
  when no death was recorded.
- `LSTVSDY` would be the study day of the last visit, counting
  days, not dates.
- `FSTALVDT` would be the first known alive date: the earlier of the
  death date and the last visit, or the one present when the other is
  missing. A calendar date and a day count have no common order, so
  the run is rejected before any data is read and no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which date the study reports, then state it in dates. Take the last
contact from the date the visit was collected on, read into a date column
(`LSTCONDT` here), not from its study day, so both sides are dates:

```yaml
- name: FSTALVDT
  type: date
  derivation:
    least:
      sources: [DTHDT, LSTCONDT]
```

When only the day count exists, keep the count as a count instead of forcing
it into a date column.
