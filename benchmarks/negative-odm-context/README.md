# Reject an Item Read for Rows Built Elsewhere

**Goal:** derive `SEX` for each subject of a subject list from the
subject's collected answer.

**Input:** a subject list naming each subject, and long-form Operational
Data Model (ODM) data with one row per collected item.

**Variables:**

- `SEX` would be the subject's answer to the sex question.

Each record is built from the subject list, so none of them was built from
collected items, and an item has no records of its own to be read from.
The request is rejected before any data is read.

## How to fix

Build the records from the collected items, one per subject, and read the
answer there:

```yaml
input:
  ODM: input/odm.csv
columns:
  - name: USUBJID
    derivation: ODM.SubjectKey
  - name: SEX
    derivation: {odm: ODM.IT.DM.SEX}
```

A subject list that decides which subjects appear is then a filter or a
lookup against those records, not the place they are built from.
