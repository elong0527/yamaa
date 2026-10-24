# Refuse a total that states an order

**Goal:** build one subject-level record per subject carrying `TOTDOSE`, the
subject's total administered dose.

**Input:** demographics with one record per subject, and exposure records
with the dose and start date of each administration.

**Variables:**

- `TOTDOSE` would be the total of the subject's administered doses.

**Note:** the request orders the exposure records by start date before
totalling them. A total always adds the records in the order they are
stored, so the stated order has nothing to act on, and the run is rejected
before any data is read.

## How to fix

Remove the order: the total adds the doses in their stored order.

```yaml
- name: TOTDOSE
  type: float
  derivation:
    aggregate:
      key: [STUDYID, USUBJID]
      expr: "SUM(EX.EXDOSE)"
```

If the order was meant to choose one record, such as the last dose, select
that record with a lookup that orders the records and keeps the last one. If
it was meant to list the doses in date order, join them into one text with
`STR_JOIN`, which reads the order.
