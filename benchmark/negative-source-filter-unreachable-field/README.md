# Reject a selection made on a value the records do not carry

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-filter-unreachable-field.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit, no review yet.

**Goal:** carry the age (AGE) for each subject the extract holds.

**Input:** EDC output in long form, one row per collected item; each
subject has a sex row and an age row.

**Variables:**

- **AGE** would be the age in whole years as collected, but the collected
  rows it selects are chosen by a subject identifier the records do not
  carry, and a selection nothing answers is rejected before any data is
  read, so no artifact is accepted.

**Standard:** SDTM | **Domain:** DM

## How to fix

Decide what distinguishes the collected rows the age comes from, then say
it in the words the records use. The collected rows carry the item they
answer and the subject they belong to under the extract's own names:

```yaml
- name: AGE
  type: int
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
