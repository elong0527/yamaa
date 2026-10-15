# Reject Broken Flag Condition

**Goal:** build `ELDFL`, a character flag that marks each subject aged 65
or older with `Y` and leaves the rest missing.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `ELDFL` would be `Y` for subjects aged 65 or older, missing otherwise.

**Note:** the flag's condition (`AGE >> 65`) is not a valid rule: `>>` is
not a comparison, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

Write the condition as a valid rule:

```yaml
flag: AGE >= 65
```
