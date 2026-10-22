# Reject a Run Report Saved Over Its Dataset

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, and keep the record of the run beside it.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected; missing when no age was
  collected.

**Note:** the record of the run is to be saved under the dataset's own file
name, `adsl.csv`. Saving one would overwrite the other, so the run is
rejected before any data is read and no artifact is accepted.

## How to fix

Give the run report a file of its own beside the dataset:

```yaml
output:
  path: adsl.csv
  columns: [STUDYID, USUBJID, AGE]
  report: adsl-report.csv
```
