# Reject a Largest Daily Dose Written as One Summary Inside Another

**Goal:** give each subject's largest total dose on any one study day
(`MAXDDOSE`) on the subject-level analysis dataset.

**Input:** subjects from the collected demographics, and collected exposure
records with the study day (`EXSTDY`) and dose given (`EXDOSE`), several per
day.

**Variables:**

- `MAXDDOSE` would be the largest of the subject's daily dose totals.

**Note:** the daily totals and their maximum are written as one summary
inside another. A summary of the subject's records returns one value, so
nothing says which records form each day's total, and the run is rejected
before any data is read; no artifact is accepted.

## How to fix

Total each day in a dataset of its own, one record per subject and study
day, and save it. In that specification, keyed by subject and study day:

```yaml
- name: DDOSE
  type: float
  label: Daily Dose
  derivation:
    aggregate:
      key: [STUDYID, USUBJID, EXSTDY]
      expr: "SUM(EX.EXDOSE)"
```

The subject-level dataset then reads the saved daily totals, linked through
`schema` to the specification that produced them, and takes the largest
with `MAX(DAILY.DDOSE)`.
