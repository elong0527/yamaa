# Describe Values Before and After a Laboratory Change Apart

**Goal:** carry each subject's glucose result, `AVAL`, with `PARAMCD` and its
analysis date `ADT`, and describe the results of each central laboratory
separately in the data-definition document.

**Input:** `spec.yaml` reads the subjects' glucose results, one row per
subject per collection date, and `project.yaml` submits the analysis dataset.

**Variables:**

- `PARAMCD` is `GLUC`, glucose.
- `ADT` is the date the sample was collected.
- `AVAL` is the glucose result in mg/dL.

**Note:** the study changed central laboratory on 2026-03-01. The first
laboratory reported whole numbers and the second one decimal, so the
data-definition document describes `AVAL` twice: once for the results
collected before that day, with no decimals, and once for those collected on
or after it, with one. A result collected on the day of the change belongs to
the second laboratory. The analysis dataset carries no variable naming the
laboratory: the descriptions select their records by date.
