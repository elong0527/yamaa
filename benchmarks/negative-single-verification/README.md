# Reject a Lone Check Not Written as a List

**Goal:** record `SEX` for each subject, one row per study and subject,
and require every subject to have one.

**Input:** collected demographics with the study and subject
identifiers and sex (`SEX`).

**Variables:**

- `SEX` would be the subject's recorded sex, required on every row.

**Note:** a column's checks are always written as a list, even when
there is only one, so that each check has one way to be written and one
position to be reported at. A single check written on its own is not a
list, and the run is rejected before any record is read.

## How to fix

Write the check as the only item of a list:

```yaml
- name: SEX
  type: str
  derivation: DM.SEX
  verifications:
    - not_missing
```

A second check is then one more item of the same list.
