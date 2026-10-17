# Reject a Second Statement of the Row Identity

**Goal:** record `AGE` for each subject, one row per study and subject.

**Input:** collected demographics with the study and subject
identifiers and age (`AGE`).

**Variables:**

- `AGE` would be the subject's age, copied from the collected records.

**Note:** the row identity is stated once. The study and subject
identifiers already have to be present and to appear once per row, so
a second requirement that they appear once repeats the first and the
run is rejected before any record is read.

## How to fix

Remove the `unique` check. The `keys` already require every
`STUDYID` and `USUBJID` pair to be present and unique, and a repeated
or missing pair fails the run as a duplicate or missing key:

```yaml
keys: [STUDYID, USUBJID]
```

Keep `unique` for a different set of columns that must not repeat, such
as a second identifier the keys do not name.
