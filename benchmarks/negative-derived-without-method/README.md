# Reject a Derived Age Group Without Its Method

**Goal:** carry each subject's age (`AGE`) and pooled age group (`AGEGR1`)
onto the subject-level analysis dataset, with the metadata a data
definition describes them by.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected.
- `AGEGR1` would be `<65` for a subject younger than 65 and `>=65`
  otherwise; missing when no age was collected.

**Note:** the age group is declared as derived, but no method states how it
is derived. A reviewer reading the data definition could not trace the
group back to the age, so the run is rejected before any data is read and
no artifact is accepted.

## How to fix

State the method beside the derived origin, in the words a reviewer needs to
reproduce the group:

```yaml
submission:
  core: Perm
  mandatory: false
  length: 4
  origins:
    - type: Derived
  method: Age group from AGE, <65 and >=65 years
```
