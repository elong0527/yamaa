# Reject a Copied Age That Also Claims to Be Derived

**Goal:** carry each subject's age (`AGE`) onto the subject-level analysis
dataset.

**Input:** collected demographics, one record per subject.

**Variables:**

- `AGE` is the age in years as demographics records it.

**Note:** the age is described both as copied unchanged from demographics
and as derived by the sponsor. A value copied from an earlier dataset has that
one provenance, so the description contradicts itself and the dataset is
rejected before any data is read.

## How to fix

The age is read unchanged from demographics, so describe it as a copy alone
and drop the method a copy does not need:

```yaml
origins:
  - type: Predecessor
    description: DM.AGE
```

If some ages are computed instead, for example from the birth date when
demographics records none, the value is no longer a plain copy: describe it
as derived, with the method that computes it.
