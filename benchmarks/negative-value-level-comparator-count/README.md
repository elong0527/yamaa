# Reject a Comparison of One Value Written for Several

**Goal:** build the analysis values of systolic blood pressure, `AVAL`, at
each visit and as each subject's average, with `PARAMCD`, `AVISIT` and
`DTYPE`, describing the observed and the averaged values apart.

**Input:** the subjects' systolic blood pressure, one row per subject per
visit.

**Variables:**

- `PARAMCD` is `SYSBP`, systolic blood pressure.
- `AVISIT` is the visit of an observed value, and `OVERALL` for an average.
- `DTYPE` is `AVERAGE` on a subject's average, and empty on an observed
  value.
- `AVAL` is the observed value, or the mean of the subject's observed values
  over all visits.

**Note:** the observed values are described as the rows whose `DTYPE` is
none of a list of values, but the list holds one value. A comparison against
one value is written as inequality, so the description is rejected before any
data is read.

## How to fix

Select the rows whose derivation type is anything but `AVERAGE` with `NE`,
which takes exactly one value:

```yaml
where: {DTYPE: {comparator: NE, values: [AVERAGE]}}
```

Keep `NOTIN` for two or more values, such as `[AVERAGE, LOCF]` when the
dataset also carries values carried forward.
