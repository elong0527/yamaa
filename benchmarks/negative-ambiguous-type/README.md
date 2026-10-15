# Reject Ambiguous Type

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
