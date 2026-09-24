# Reject Undeclared Field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-undeclared-field.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the age in `AGE` for each subject.

**Input:** collected demographics carrying age (`AGE`) for each
subject.

**Variables:**

- `AGE` would be the age, copied from the collected age field, but
  the copy asks for a field named `AGEYRS`, which the collected
  demographics do not have, so the run is rejected before any data
  is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide which collected field holds the age, then name it exactly. When the
source carries the age under its own name, copy that field:

```yaml
- name: AGE
  type: int
  label: Age
  derivation:
    source: DM.AGE
```

When the source truly lacks the field, add it to the governed source first
rather than pointing at a name nothing declares.
