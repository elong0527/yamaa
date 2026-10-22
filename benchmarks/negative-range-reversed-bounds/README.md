# Reject an Age Check Whose Limits Are Reversed

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset, and confirm that every age lies within the protocol's limits.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` would be the age in years as collected, between 18 and 120.

**Note:** the age limits were entered the wrong way round: at least 120 and
at most 18. No age satisfies both, so the check could never pass, and the
run is rejected before any data is read; no artifact is accepted.

## How to fix

State the protocol's lower limit as the minimum and its upper limit as the
maximum:

```yaml
verifications:
  - range: {min: 18, max: 120}
```
