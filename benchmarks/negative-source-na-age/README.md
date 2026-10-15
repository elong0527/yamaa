# Reject NA Age

**Goal:** carry `AGE` unchanged for analysis, keeping the collected
age for each subject.

**Input:** collected demographics records holding `AGE`.

**Variables:**

- `AGE` would contain the collected age copied from the collected
  records, but a stored age of `NA` is not a number, so the run stops
  while reading the stored value and no artifact is accepted.

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
