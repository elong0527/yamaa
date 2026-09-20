# Reject an age recorded as NA

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-missing-sentinel.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** carry `AGE` unchanged for analysis, keeping the collected
age for each subject.

**Input:** collected demographics records holding `AGE`.

**Variables:**

- `AGE` would contain the collected age copied from the collected
  records, but no row is produced because `NA` is not a number and
  the run stops while reading the stored value, so no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Leave the field empty when an age was not collected, and keep reading the
field as a number:

```yaml
input:
  DM:
    path: input/dm.csv
    types:
      AGE: int
```

If the code carries a meaning worth keeping, such as an age withheld rather
than never taken, read the field as text and map the code to a value the
study defines.
