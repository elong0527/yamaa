# Reject a Copied Age That Does Not Name Its Source

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, with the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years, copied unchanged from demographics.

**Note:** the age is declared as copied from an earlier dataset, but the
declaration does not name the dataset and variable it was copied from. A
reviewer could not find the source of the value, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Name the source as `<dataset>.<variable>`, the variable this column reads:

```yaml
origins:
  - type: Predecessor
    description: DM.AGE
```
