# Reject a Role the SDTM Model Does Not Publish

**Goal:** carry each subject's age (`AGE`) with its unit (`AGEU`), and
describe the role the unit plays in the demographics domain.

**Input:** collected demographics with the study and subject identifiers
and each subject's age in years.

**Variables:**

- `AGE` would be the subject's age in years, as collected.
- `AGEU` would be the unit of that age, `YEARS` on every record.

**Note:** the SDTM model publishes nine variable roles: Identifier, Topic,
Timing and Rule, and the Grouping, Result, Synonym, Record and Variable
Qualifiers. The role written for the age unit, `Qualifier`, names the group
the five qualifier roles belong to rather than one role, so the request is
rejected before any data is read, and no artifact is accepted.

## How to fix

Name the one role the variable plays. The age unit qualifies the age
itself, which the SDTM model calls a Variable Qualifier:

```yaml
submission:
  core: Exp
  role: Variable Qualifier
  length: 5
  origin:
    type: Assigned
    source: Sponsor
```

A role is written exactly as the SDTM model spells it:
`Variable qualifier` is rejected the same way.
