# Reject Dose Periods Placed by the Subject's First Dose Date

**Goal:** give each subject the date of their first dose in treatment period
2 (`TR02SDT`).

**Input:** the subjects with the date of their first dose, their dosing
records with a start date, and each subject's treatment periods, each from a
first to a last day.

**Variables:**

- `TR02SDT` would be the start date of the subject's earliest dose whose
  period is period 2, and blank for a subject with no dose in period 2.

**Note:** each dose is placed in a period while that dose is looked at, and
only the dose itself is known then. The period is chosen by the subject's
first dose date instead, a value of the subject rather than of the dose, so
the run is rejected before any data is read and no artifact is accepted.

## How to fix

Place each dose by its own start date, which the dose record holds:

```yaml
between: {value: EXSTDT, lower: APERSDT, upper: APEREDT}
```

`EX.EXSTDT` names the same date. The subject's first dose date would place
every dose of the subject in the same period.
