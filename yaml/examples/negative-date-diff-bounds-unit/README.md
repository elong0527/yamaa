# Reject endpoint counting beside a month count

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-date-diff-bounds-unit.html)

**Goal:** carry the exposure start (`STDT`) and end (`ENDT`) dates
forward for each subject and count whole months between them in
`DURM`.

**Input:** collected demographics (DM) records carrying a start
date (`STDT`) and an end date (`ENDT`), one record for each
subject.

**Variables:**

- `STDT` would be the start date of the exposure, taken from
  `STDT` in the demographics input.
- `ENDT` would be the end date of the exposure, taken from `ENDT`
  in the demographics input.
- `DURM` would be whole months between `STDT` and `ENDT`,
  counting both endpoints, which has no meaning beside a month
  count. The request is rejected before any data is read and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide whether the study counts days or whole months, then state only that. A
month count stands on its own with no endpoint adjustment:

```yaml
- name: DURM
  type: int
  derivation:
    date_diff:
      start: STDT
      end: ENDT
      unit: month
```

When the study counts days with both endpoints included, keep `unit: day`
beside `bounds: inclusive` instead.
