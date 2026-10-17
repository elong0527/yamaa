# Reject an Age Band Chosen by Another Dataset's Age

**Goal:** attach the reference range limits (`LBSTNRLO`, `LBSTNRHI`) to
each laboratory result from the age band that holds the subject's age.

**Input:** laboratory results carrying a test code (`LBTESTCD`) and a
numeric result (`LBSTRESN`), demographics carrying each subject's age
(`AGE`), and a range table giving each test code's lower and upper limits
for an age band from a lowest (`AGELO`) to a highest (`AGEHI`) age.

**Variables:**

- `LBSTNRLO` and `LBSTNRHI` would be the limits of the band for the
  result's test code whose ages, both ends inclusive, hold the subject's
  age, and blank when no band does.

**Note:** a band is chosen by a value of the laboratory row itself: a
field of the laboratory record the row was built from, or a column built
for that row. The age is taken directly from demographics, a different
dataset, so it is not a value of the laboratory row. The run is rejected
before any data is read; no artifact is accepted.

## How to fix

Decide which age selects the band; usually it is the subject's age as
recorded in demographics. Bring that age onto each laboratory row as its
own column, matched to the result by study and subject, and choose the
band by that column:

```yaml
columns:
  - name: AGE
    type: int
    label: Age
    derivation: DM.AGE
```

```yaml
between: {value: AGE, lower: AGELO, upper: AGEHI}
```

Leave `AGE` out of the published columns when the artifact should not
carry it.
