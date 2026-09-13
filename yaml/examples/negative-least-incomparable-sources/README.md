# ADaM ADSL: reject an earliest-alive date taken from a day number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-least-incomparable-sources.html)

This example uses collected demographics to record one row per subject:

- `DTHDT` is the date of death, missing for subjects still followed.
- `LSTVSDY` is the study day of the last visit, counting days, not dates.
- `FSTALVDT` keeps the earlier of the two as the first known-alive date.

A calendar date and a day count have no common order: no implementation
may compare them, so the specification is rejected before any data is
read and no artifact is accepted.

## How to fix

Decide which date the study reports, then state it in dates. When the
last contact is only known as a study day, convert it against the
reference start date first so both sides are dates:

```yaml
- name: FSTALVDT
  type: date
  derivation:
    least:
      sources: [DTHDT, LSTCONDT]
```

When only the day count exists, keep the count as a count instead of
forcing it into a date column.
