# Reject Numeric Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-flag-payload.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y`,
leaving younger subjects missing.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `AGE` is the collected numeric age.
- `ELDFL` would be `Y` for subjects aged 65 or older, missing otherwise.

**Note:** the flag's condition is written as the bare number `65` rather
than a test on age, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

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
