# Reject a Tabulation Core Designation on an Analysis Column

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, with the metadata a data definition describes it by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected; missing when no age was
  collected.

**Note:** the age is described as Expected, a core designation of the
tabulation model. The analysis model publishes Required, Conditionally
required and Permissible only, so the designation names nothing its
standard defines, and the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Use the designation the analysis implementation guide publishes for the
variable. Age is a required variable of the subject-level dataset:

```yaml
submission:
  core: Req
  mandatory: false
```

`mandatory` states separately whether every record must carry an age.
