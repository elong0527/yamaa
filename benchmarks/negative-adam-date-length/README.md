# Reject a Length Declared on an Analysis Date

**Goal:** carry each subject's date of first dose (`TRTSDT`) onto the
subject-level analysis dataset, with the metadata a data definition
describes it by.

**Input:** collected demographics with the date of first exposure
(`RFXSTDT`), one record per subject.

**Variables:**

- `TRTSDT` would be the date of first exposure to treatment.

**Note:** the date declares a length. A date is submitted in the fixed form
its model gives it, so a length restates a fact no declaration can change
and could disagree with it; the run is rejected before any data is read and
no artifact is accepted.

## How to fix

Remove the length; the analysis model gives every date the length of a SAS
numeric date:

```yaml
submission:
  core: Cond
  mandatory: false
  origins:
    - type: Predecessor
      description: DM.RFXSTDT
```
