# Refuse a total of distinct doses

**Goal:** build one subject-level record per subject carrying `TOTDOSE`, the
subject's total administered dose.

**Input:** demographics with one record per subject, and exposure records
with the dose of each administration.

**Variables:**

- `TOTDOSE` would be the total of the subject's administered doses.

**Note:** the request totals each different dose amount once. Two
administrations of the same amount are two doses, and totalling the amount
once would report less than the subject received, so the run is rejected
before any data is read.

## How to fix

Total every administration, so that a repeated amount counts each time it was
given:

```yaml
- name: TOTDOSE
  type: float
  derivation:
    aggregate:
      key: [STUDYID, USUBJID]
      expr: "SUM(EX.EXDOSE)"
```

If a repeated record is an administration entered twice by mistake, correct
the exposure data. If the number of different dose levels is wanted instead,
count them: `COUNT(EX.EXDOSE)` with `distinct: true`.
