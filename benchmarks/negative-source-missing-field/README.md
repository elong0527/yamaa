# Reject Missing Field

**Goal:** carry the age (AGE) for each subject the extract holds.

**Input:** EDC output in long form, one row per collected item; each
subject has a sex row and an age row.

**Variables:**

- **AGE** would be the age in whole years as collected. The collected
  rows it reads are also chosen by subject, under the name `USUBJID`,
  which those rows do not carry: they hold the subject as `SubjectKey`.
  The run is therefore rejected before any data is read, and no
  artifact is accepted.

## How to fix

Decide what distinguishes the collected rows the age comes from, then say
it in the words the records use. The collected rows carry the item they
answer and the subject they belong to under the extract's own names:

```yaml
- name: AGE
  type: int
  label: Age
  derivation:
    source:
      filter: ODM.ItemOID = 'IT.DM.AGE'
      variable: ODM.Value
```

The subject needs no mention: each record is built for one subject and
reads only the rows that subject was collected under.

When the intent was to leave some subjects without an age, state that as
a rule over the built record rather than over the collected rows, for
example by mapping a collected exclusion flag.
