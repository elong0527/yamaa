# Reject an Analysis Date Submitted as Text

**Goal:** carry each subject's date of first dose (`TRTSDT`) onto the
subject-level analysis dataset, with the metadata a data definition
describes it by.

**Input:** collected demographics with the date of first exposure
(`RFXSTDT`), one record per subject.

**Variables:**

- `TRTSDT` would be the date of first exposure to treatment.

**Note:** the date is described for submission as an ISO 8601 date. An
analysis dataset submits a date as the number of days since 1 January 1960,
displayed as `DATE9.`, so the declared type is one the analysis model does
not submit a date as, and the run is rejected before any data is read; no
artifact is accepted.

## How to fix

Leave the submission type out: an analysis date is submitted as an integer
with the `DATE9.` display, which the model supplies:

```yaml
submission:
  core: Cond
  mandatory: false
  origins:
    - type: Predecessor
      description: DM.RFXSTDT
```
