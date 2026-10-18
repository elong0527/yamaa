# Reject a Subject Record Identified by the Subject's Collected Sex

**Goal:** build one demographics record per subject from the collected study
data, with the subject's sex in `SEX`.

**Input:** collected study data in operational data model form, one record
per collected item, with the item's identifier, value and position in the
study.

**Variables:**

- `SEX` would be the sex collected for the subject.

**Note:** the subject's records are found by the values that identify them,
and each collected item supplies those values by itself. The sex is part of
what identifies a record here, but it is an item of its own, found among the
subject's items only once the subject's records are found. The run is
rejected before any data is read and no artifact is accepted.

## How to fix

The subject alone identifies the record. Keep the sex as a value of the
record, not part of its identity:

```yaml
keys: [STUDYID, USUBJID]
```

`SEX` keeps its `odm` derivation as an ordinary column.
