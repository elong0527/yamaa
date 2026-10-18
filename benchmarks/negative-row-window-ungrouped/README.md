# Reject an Order of First Doses Taken From Single Administrations

**Goal:** build one record per subject from the subject's exposure records,
with the date of first exposure to treatment in `TRTSDT` and the subject's
place in the order in which the study's subjects started treatment in
`DOSEORD`.

**Input:** exposure records, one per administration, carrying the start date
of each administration (`EXSTDTC`).

**Variables:**

- `TRTSDT` would be the earliest start date among the subject's
  administrations.
- `DOSEORD` would number the study's subjects from 1 in order of their first
  dose: the subject who started treatment first gets 1.

**Note:** each subject's record summarizes all of the subject's
administrations, so a single administration's start date is not a value of
that record: a subject with several administrations has several such dates.
The order of first doses is taken from that date, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Order the subjects by a value of the subject's record. `TRTSDT` already holds
the earliest start date, so number the subjects by it:

```yaml
DOSEORD:
  row_number:
    window:
      group_by: [STUDYID]
      order_by: [TRTSDT]
```

Add a tie-breaking field when two subjects can start treatment on the same
day.
