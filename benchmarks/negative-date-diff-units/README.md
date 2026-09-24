# Reject Mixed Units

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-date-diff-units.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the exposure start (`STDT`) and end (`ENDT`) dates
forward for each subject and count whole months between them in
`DURM`.

**Input:** collected demographics (DM) records carrying a start
date (`STDT`) and an end date (`ENDT`), one record for each
subject.

**Variables:**

- `DURM` would be the number of whole months from `STDT` to `ENDT`, but it
  also asks to count both end dates, which is defined only for a count of
  days. The request is rejected before any data is read and no artifact is
  accepted.

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
