# Reject Undeclared Field

**Goal:** carry the age in `AGE` for each subject.

**Input:** collected demographics carrying age (`AGE`) for each
subject.

**Variables:**

- `AGE` would be the age, copied from the collected age field, but
  the copy asks for a field named `AGEYRS`, which the collected
  demographics do not have, so the run is rejected before any data
  is read and no artifact is accepted.

## How to fix

Decide which collected field holds the age, then name it exactly. When the
source carries the age under its own name, copy that field:

```yaml
- name: AGE
  type: int
  label: Age
  derivation: DM.AGE
```

When the source truly lacks the field, add it to the governed source first
rather than pointing at a name nothing declares.
