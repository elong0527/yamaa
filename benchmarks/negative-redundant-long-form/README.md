# Reject a Full Form That Only Repeats Its Short Spelling

**Goal:** record `WTMAX`, each subject's heaviest collected weight.

**Input:** collected demographics with the study and subject
identifiers, and vital signs carrying several weights (`WEIGHT`) per
subject.

**Variables:**

- `WTMAX` would be the largest weight collected for the subject.

**Note:** a value has one spelling. The summary is written in its full
form, but that form holds nothing beyond the formula itself, which the
short spelling already says; the two would mean the same thing, so the
run is rejected before any record is read.

## How to fix

Write the formula as the short spelling:

```yaml
derivation:
  aggregate: "MAX(VS.WEIGHT)"
```

Keep the full form only when it adds something the short spelling cannot
say, such as its own match key or a filter:

```yaml
derivation:
  aggregate:
    expr: "MAX(VS.WEIGHT)"
    filter: "VS.WEIGHT > 0"
```
