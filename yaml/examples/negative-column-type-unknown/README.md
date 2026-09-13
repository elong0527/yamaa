# Reject an analysis value with an ambiguous numeric type

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-column-type-unknown.html)

**Goal:** attempt one laboratory analysis record per collected
laboratory result, carrying `AVAL` for the numeric analysis value.

**Input:** collected laboratory results with the result in
standard units (`LBSTRESN`).

**Variables:**

- `AVAL` would be the numeric analysis value, copying `LBSTRESN`.
  A result distinguishes whole numbers from fractional numbers,
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
  derivation:
    source: LB.LBSTRESN
```

The allowed column types are `str`, `int`, `float`, `date`, and `datetime`.
