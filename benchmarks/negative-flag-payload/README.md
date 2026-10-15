# Reject Numeric Flag

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y`,
leaving younger subjects missing.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `AGE` is the collected numeric age.
- `ELDFL` would be `Y` for subjects aged 65 or older, missing otherwise.

**Note:** the flag's condition is written as the bare number `65` rather
than a test on age, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

State the condition as a test on age:

```yaml
flag: AGE >= 65
```

or name it inside the flag:

```yaml
flag:
  condition: AGE >= 65
```
