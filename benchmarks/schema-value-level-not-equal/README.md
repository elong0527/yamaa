# Describe Observed and Averaged Values Apart

**Goal:** build the analysis values of systolic blood pressure, `AVAL`, at
each visit and as each subject's average, with `PARAMCD`, `AVISIT` and
`DTYPE`, and give the observed and the averaged values their own definitions
in the data-definition document.

**Input:** `spec.yaml` reads the subjects' systolic blood pressure, one row
per subject per visit, and `project.yaml` submits the analysis dataset.

**Variables:**

- `PARAMCD` is `SYSBP`, systolic blood pressure.
- `AVISIT` is the visit of an observed value, and `OVERALL` for an average.
- `DTYPE` is `AVERAGE` on a subject's average, and empty on an observed
  value.
- `AVAL` is the observed value, or the mean of the subject's observed values
  over all visits.

**Note:** the data-definition document describes `AVAL` twice: once for the
rows whose `DTYPE` is anything but `AVERAGE`, which takes in the observed
rows, where it is empty, copied from the collected value; and once for the
averaged rows, derived by the mean. The two descriptions select no common
row.
