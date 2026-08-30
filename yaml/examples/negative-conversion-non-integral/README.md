# ADaM ADVS: reject a pulse rate recorded between whole beats

This example uses collected pulse rates to attempt one record per measurement:

- `AVAL` is the pulse in beats per minute, which the analysis holds as a whole
  number.

One rate was averaged over a half-minute count and recorded with a fraction.
Dropping the fraction and moving to the nearest whole number disagree, and both
report a rate that was not measured, so the run must fail and no artifact is
accepted.

## How to fix

If fractional pulse rates are valid for the analysis, preserve the collected
precision by declaring the result as `float`:

```yaml
- name: AVAL
  type: float
  derivation:
    source: VS.VSSTRESN
```

If the result must be an integer, the specification must choose an explicit
rule such as `FLOOR`, `CEIL`, or `TRUNC` in a `compute` expression; conversion
will not choose a rounding rule implicitly.
