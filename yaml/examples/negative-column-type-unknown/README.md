# ADaM ADLB: reject an analysis value with an ambiguous numeric type

This example uses collected laboratory results to attempt one record per
result:

- `AVAL` is the numeric analysis value of the result.

A result distinguishes whole numbers from fractional numbers, but the declared
type says only that the value is a number. Choosing either representation would
invent a precision decision the specification did not make, so the run must
fail and no artifact is accepted.

## How to fix

Choose the type from the measurement contract. Use `int` only when fractions
are impossible; otherwise use `float`:

```yaml
- name: AVAL
  type: float
  derivation:
    source: LB.LBSTRESN
```

The allowed column types are `str`, `int`, `float`, and `date`.
