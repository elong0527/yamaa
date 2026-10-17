# Reject Mixed Units

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

## How to fix

Decide whether the study counts days or whole months, then state only that. A
month count stands on its own with no endpoint adjustment:

```yaml
- name: TRTDURM
  type: int
  derivation:
    diff:
      type: date
      start: TRTSDT
      end: TRTEDT
      unit: month
```

When the study counts days with both endpoints included, keep `unit: day`
beside `bounds: inclusive` instead.
