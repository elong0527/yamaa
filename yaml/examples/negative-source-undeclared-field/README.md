# Reject a copy from an undeclared field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-undeclared-field.html)

**Goal:** carry the age in `AGE` for each subject.

**Input:** collected demographics carrying age (`AGE`) for each
subject.

**Variables:**

- `AGE` would be the age, copied from the collected age field, but
  the collected demographics declare no field by the name the copy
  asks for, so the run is rejected before any data is read and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which collected field holds the age, then name it exactly. When the
source carries the age under its own name, copy that field:

```yaml
- name: AGE
  type: int
  derivation:
    source: DM.AGE
```

When the source truly lacks the field, add it to the governed source first
rather than pointing at a name nothing declares.
