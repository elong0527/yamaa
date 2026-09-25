# Reject Mixed Units

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-date-diff-units.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the exposure start (`TRTSDT`) and end (`TRTEDT`) dates
forward for each subject and count whole months between them in
`TRTDURM`.

**Input:** collected demographics (DM) records carrying a start
date (`TRTSDT`) and an end date (`TRTEDT`), one record for each
subject.

**Variables:**

- `TRTDURM` would be the number of whole months from `TRTSDT`
  to `TRTEDT`, but it also asks to count both end dates, which is
  defined only for a count of days. The request is rejected before
  any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide whether the study counts days or whole months, then state only that. A
month count stands on its own with no endpoint adjustment:

```yaml
- name: TRTDURM
  type: int
  derivation:
    date_diff:
      start: TRTSDT
      end: TRTEDT
      unit: month
```

When the study counts days with both endpoints included, keep `unit: day`
beside `bounds: inclusive` instead.
