# Reject taking the subject identifier from the subject-level dataset again

**Goal:** give every adverse event record its subject's age (`AGE`) and sex
(`SEX`) from the subject-level analysis dataset (ADSL).

**Input:** adverse event records carrying the unique subject identifier
(`USUBJID`) and the reported term (`AETERM`), and the ADSL that the study's
subject-level specification builds from collected demographics.

**Variables:**

- `AGE` and `SEX` would be the subject's values in ADSL, repeated on each of
  their events.

**Note:** the event records already carry the subject identifier, so taking
it from ADSL as well would declare two variables of one name. The run is
refused before any record is read instead of choosing between them.

## How to fix

Keep the identifier the event records carry, which is also the one each
event is matched to its subject's ADSL record on, and list only the
subject-level variables in the copy entry:

```yaml
  - copy:
      dataset: ADSL
      columns: [AGE, SEX]
```
