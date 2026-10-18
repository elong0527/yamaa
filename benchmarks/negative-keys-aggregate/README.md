# Reject a Subject Record Identified by the Subject's Highest Dose

**Goal:** build one record per subject from the exposure records, with the
highest dose the subject received in `MAXDOSE`.

**Input:** exposure records, one per administration, carrying the study day
(`EXSTDY`) and the dose given (`EXDOSE`).

**Variables:**

- `MAXDOSE` would be the highest dose among the subject's administrations.

**Note:** the subject's records are found by the values that identify them,
and each administration supplies those values by itself. The highest dose
is part of what identifies a record here, but it summarizes all of the
subject's administrations, which are known only once the subject's records
are found. The run is rejected before any data is read and no artifact is
accepted.

## How to fix

The subject alone identifies the record. Keep the highest dose as a value of
the record, not part of its identity:

```yaml
keys: [STUDYID, USUBJID]
```

`MAXDOSE` keeps its `aggregate` derivation as an ordinary column.
