# Reject Bare Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-flag-payload.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y`.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `ELDFL` would be `Y` for subjects aged 65 or older, missing otherwise.

**Note:** the flag is written as a bare string (`flag: AGE >= 65`)
instead of a mapping naming its condition, so the run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Name the condition inside the flag:

```yaml
flag:
  condition: AGE >= 65
```
