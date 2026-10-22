# Reject a Uniqueness Check Over Events Chosen Per Subject

**Goal:** carry the disposition event that follows each subject's last dose
(`EOSSTT`) onto the subject-level analysis dataset.

**Input:** collected demographics with the study day of the last dose
(`TRTEDY`), one record per subject, and collected disposition events with
their study day (`DSSTDY`).

**Variables:**

- `TRTEDY` would be the study day of the last dose as collected.
- `EOSSTT` would be the disposition event recorded after the last dose;
  missing when no event follows it.

**Note:** the events are checked to be one per subject before any record is
built, but which events count is decided by comparing each with the record
being built, its last dose day. A check made once cannot cover events chosen
record by record, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

Choose the events by their own fields when they must be checked once, for
example by the disposition category or term, and leave the comparison with
the last dose day to a filter without the check:

```yaml
- id: POSTTRT
  dataset: DS
  key: [STUDYID, USUBJID]
  filter: "DS.DSSTDY > DM.TRTEDY"
  order_by: [DS.DSSTDY]
  keep: first
```

Here the first event after the last dose is kept for each subject, so no
uniqueness check is needed.
