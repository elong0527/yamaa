# Reject a Dose Period Looked Up from the Dose That Builds the Row

**Goal:** give each dose that was given its treatment period (`APERIOD`).

**Input:** dosing records with the dose given and its start date, and each
subject's treatment periods, each from a first to a last day.

**Variables:**

- `APERIOD` would be the number of the subject's period whose first and last
  days, both inclusive, hold the dose's start date, and blank when no period
  does.

**Note:** each row is built from one dose, and the period is looked up once
for every row that reads it. Such a look-up cannot read the dose a row is
built from, so this is not supported, and the run stops before any data is
read; no artifact is accepted.

## How to fix

Look the period up where the row is built, so it reads that row's dose:

```yaml
APERIOD:
  source:
    variable: PERIODS.APERIOD
    between: {value: DOSED.EXSTDT, lower: APERSDT, upper: APEREDT}
```

The period table then needs its key, `key: [STUDYID, USUBJID]`, to join each
dose to its subject's periods.
