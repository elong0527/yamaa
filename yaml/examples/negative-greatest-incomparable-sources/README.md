# Reject a last alive date from mixed scales

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-greatest-incomparable-sources.html)

**Goal:** carry `DTHDT` and `LSTVSDY` through and take the later
of the two in `LSTALVDT` as the last known alive date.

**Input:** collected demographics carrying the date of death
(`DTHDTC`) and the last-visit study day (`LSTVSDY`).

**Variables:**

- `DTHDT` would be the date of death, carried over from `DTHDTC`,
  and missing when no death date was collected.
- `LSTVSDY` would be the study day of the last visit, counted
  from the first dose and carried over from `LSTVSDY`.
- `LSTALVDT` would be the later of `DTHDT` and `LSTVSDY` as the
  last known alive date, but a calendar date and a day number
  share no common order, so the run is rejected before any data
  is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Put both candidates on the same scale before comparing them. For example,
carry the actual last-visit date in the source and compare two date columns:

```yaml
- name: LSTVSDT
  type: date
  derivation:
    source: DM.LSTVSDTC

- name: LSTALVDT
  type: date
  derivation:
    greatest:
      sources: [DTHDT, LSTVSDT]
```

If only `LSTVSDY` is available, derive its calendar date from the study-day
reference under a separately stated rule before using `greatest`.
