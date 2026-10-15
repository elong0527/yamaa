# Reject Mixed Units

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-date-diff-units.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build a subject-level record carrying the exposure start
(`TRTSDT`) and end (`TRTEDT`) dates and counting whole months between
them in `TRTDURM`.

**Input:** collected demographics (DM) records carrying an exposure
start date (`TRTSDT`) and an exposure end date (`TRTEDT`), one record
for each subject.

**Variables:**

- `TRTSDT` would be the exposure start date, taken from DM as
  collected.
- `TRTEDT` would be the exposure end date, taken from DM as collected.
- `TRTDURM` would be the whole-month count from `TRTSDT` to `TRTEDT`,
  but it also asks to count both endpoints, which is defined only for
  a count of days.

**Note:** counting both endpoints beside a month count has no meaning:
inclusive bounds answer "how many days, endpoints included", which has
no whole-month reading. The request is rejected before any data is
read, so no output is produced.

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
