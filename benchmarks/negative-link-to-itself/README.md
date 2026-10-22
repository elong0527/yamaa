# Reject a Saved Dataset Typed by the File That Reads It

**Goal:** carry each subject's age (`AGE`) and the age recorded at the
first data cut (`AGECUT1`) onto the subject-level analysis dataset.

**Input:** collected demographics, one record per subject, and the
subject-level dataset saved at the first data cut.

**Variables:**

- `AGE` would be the age in years as collected.
- `AGECUT1` would be the age the first data cut recorded for the subject;
  missing for a subject enrolled after that cut.

**Note:** the dataset saved at the first data cut is described as having
the columns of the very file that reads it, so the columns it may hold
depend on the result being built. That description cannot be read before
the build, so the run is rejected before any data is read and no artifact
is accepted.

## How to fix

Describe the saved dataset by the file that produced it at the first data
cut, kept unchanged beside this one, for example `spec_cut1.yaml`:

```yaml
input:
  PREV:
    path: input/adsl_cut1.csv
    schema: spec_cut1.yaml
    key: [STUDYID, USUBJID]
```

When no such file was kept, declare the saved columns' types directly with
`types` instead of a `schema` link.
