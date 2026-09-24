# Reject Flag Without Missing Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-flag-missing-value.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `ELDFL` to mark each subject aged 65 or older with `Y` and
every younger subject with `N`.

**Input:** collected demographics carrying numeric age (`AGE`), with one
subject whose age was not collected.

**Variables:**

- `AGE` is the collected age, missing where it was not collected.
- `ELDFL` would be `Y` for subjects aged 65 or older and `N` for younger
  subjects.

**Note:** the flag names `N` for a false condition (`false_value`) but
not what a subject with no recorded age receives (`missing_value`), so the
run is rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

State what an unknown age returns. To give `N` to every subject not known
to be 65 or older:

```yaml
flag:
  condition: AGE >= 65
  false_value: N
  missing_value: N
```

Write `missing_value: null` instead to leave the flag missing when the age
is unknown.
