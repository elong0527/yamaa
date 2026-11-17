# Reject Two Laboratory Descriptions That Share the Day of the Change

**Goal:** carry each subject's glucose result, `AVAL`, with `PARAMCD` and its
analysis date `ADT`, describing the results of each central laboratory
separately in the data-definition document.

**Input:** the subjects' glucose results, one row per subject per collection
date.

**Variables:**

- `PARAMCD` is `GLUC`, glucose.
- `ADT` is the date the sample was collected.
- `AVAL` is the glucose result in mg/dL.

**Note:** the first laboratory's results are described as those collected on
or before 2026-03-01 and the second's as those collected on or after it, so a
result collected on the day of the change would carry both descriptions. Each
result must carry at most one, so the two are rejected before any data is
read, whether or not a sample was collected that day.

## How to fix

Decide which laboratory measured the samples of the day of the change, and
exclude that day from the other description. When the second laboratory took
over that morning:

```yaml
- id: LAB_A
  where:
    PARAMCD: [GLUC]
    ADT: {comparator: LT, values: ['2026-03-01']}
```
