# Reject Ambiguous Type

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-ambiguous-type.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt one laboratory analysis record per collected
laboratory result, carrying `AVAL` for the numeric analysis value.

**Input:** collected laboratory results with the result in
standard units (`LBSTRESN`).

**Variables:**

- `AVAL` would be the numeric analysis value, copying `LBSTRESN`.
  Whole and fractional numbers are stored as different kinds of value,
  but the declared kind says only `number`. Choosing either
  representation would invent a precision decision the
  declaration did not make, so the run is rejected before any
  data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Choose the type from the measurement contract. Use `int` only when fractions
are impossible; otherwise use `float`:

```yaml
- name: AVAL
  type: float
  label: Analysis Value
  derivation: LB.LBSTRESN
```

The allowed column types are `str`, `int`, `float`, `date`, and `datetime`.
