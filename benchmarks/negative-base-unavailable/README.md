# Reject Two Inputs Without a Base Dataset

**Goal:** build one record per subject carrying the age (`AGE`) from the
demographics and the planned arm (`ARM`) from the treatment assignments.

**Input:** collected demographics and planned treatment assignments, each
holding one record per subject.

**Variables:**

- `AGE` would be the age in years as collected.
- `ARM` would be the planned arm of the subject's treatment assignment.

**Note:** both inputs hold subject records, and nothing says which one
supplies the subjects of the dataset. A subject present in only one of them
would appear or not depending on that choice, so the run is rejected before
any data is read and no artifact is accepted.

## How to fix

Decide which input lists the subjects of the dataset, and name it as the
base. The demographics list every subject; the treatment assignments are
read for each of them:

```yaml
base: DM
```
