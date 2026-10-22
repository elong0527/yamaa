# Reject a Dataset Name With a Hyphen

**Goal:** carry each subject's age (`AGE`) onto a second version of the
subject-level analysis dataset, named `ADSL-V2`.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected.

**Note:** a dataset name holds only letters, digits and underscores, and
does not start with a digit, so that every reader and every transport format
can name it. `ADSL-V2` holds a hyphen, so the run is rejected before any data
is read and no artifact is accepted.

## How to fix

Name the dataset with letters, digits and underscores only. A submitted
analysis dataset keeps its standard name, and the version belongs in the
file name or the study's records:

```yaml
domain: ADSL
```
