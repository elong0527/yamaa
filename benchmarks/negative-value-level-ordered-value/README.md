# Reject a Laboratory Change Date Written as a SAS Date

**Goal:** carry each subject's glucose result, `AVAL`, with `PARAMCD` and its
analysis date `ADT`, describing the results of each central laboratory
separately in the data-definition document.

**Input:** the subjects' glucose results, one row per subject per collection
date.

**Variables:**

- `PARAMCD` is `GLUC`, glucose.
- `ADT` is the date the sample was collected.
- `AVAL` is the glucose result in mg/dL.

**Note:** the results of the first laboratory are described as those
collected before `01MAR2026`. That is how SAS displays a date, not a date
itself, so the analysis date cannot be compared with it and the description
is rejected before any data is read.

## How to fix

Write the day of the change as an ISO 8601 date, as the second laboratory's
description already does:

```yaml
where:
  PARAMCD: [GLUC]
  ADT: {comparator: LT, values: ['2026-03-01']}
```
