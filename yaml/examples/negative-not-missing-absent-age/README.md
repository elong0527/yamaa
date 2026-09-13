# Reject a missing age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-not-missing-absent-age.html)

**Goal:** record `AGE` for every subject from the collected
demographics, requiring a value for each subject.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGE` would be the subject's age, copied from the collected
  records.

A subject with no recorded age leaves it missing, so the
completed-dataset check fails it and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide whether the study can proceed without the value, then either correct
the data or loosen the rule. When the age exists on the case report form,
correct it at the governed source and rerun. When absence is genuinely
possible, drop the rule and let the column stay missing:

```yaml
- name: AGE
  type: int
  derivation:
    source: DM.AGE
```

Do not fill the gap with a placeholder age, which reports a value nobody
recorded.
