# Reject a Numeric Age Group Bound to a Text Codelist

**Goal:** carry each subject's age (`AGE`) and numeric pooled age group
(`AGEGR1N`) onto the subject-level analysis dataset, with the group
restricted to the study's codelist for it.

**Input:** collected demographics, one record per subject, and the study's
codelists in the project file: a text list of the age groups and a numeric
list of their codes.

**Variables:**

- `AGE` would be the age in years as collected.
- `AGEGR1N` would be 1 for a subject younger than 65 and 2 otherwise;
  missing when no age was collected.

**Note:** the numeric age group is bound to the text list of the age groups,
whose values are `<65` and `>=65`. A number can never be one of those
values, so the binding is a mistake rather than a check, and the run is
rejected before any data is read; no artifact is accepted.

## How to fix

Bind the numeric group to the numeric list of its codes:

```yaml
submission:
  core: Perm
  mandatory: false
  length: 1
  codelist: AGEGR1N
  origins:
    - type: Derived
  method: 1 for an age below 65 years, 2 for 65 years or above
```
