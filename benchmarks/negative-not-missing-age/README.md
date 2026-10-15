# Reject Missing Age

**Goal:** record `AGE` for every subject from the collected
demographics, requiring a value for each subject.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGE` would be the subject's age, copied from the collected
  records.

**Note:** a subject with no recorded age leaves it missing, so the
check on the completed dataset fails and no artifact is accepted.

## How to fix

Decide whether the study can proceed without the value, then either correct
the data or loosen the rule. When the age exists on the case report form,
correct it at the governed source and rerun. When absence is genuinely
possible, drop the `not_missing` check and let that subject's age stay
missing:

```yaml
- name: AGE
  type: int
  label: Age
  derivation: DM.AGE
```

Do not fill the gap with a placeholder age, which reports a value nobody
recorded.
