# ADaM ADLB: reject a percent change from a zero baseline

This example uses collected laboratory results with their baseline values to
attempt one record per subject and parameter:

- `AVAL` is the collected result and `BASE` the baseline it is compared with;
- `PCHG` is the change from baseline as a percentage of it.

One subject's baseline is zero, so the percentage has no value. Reporting it as
absent would hide a rule the specification never stated, so the run must fail
and no artifact is accepted.

## How to fix

If the intended result is missing when `BASE` is zero, guard the denominator
explicitly with `NULLIF`:

```yaml
derivation:
  compute:
    expr: "100 * (AVAL - BASE) / NULLIF(BASE, 0)"
```

`NULLIF(BASE, 0)` returns missing for a zero baseline. Missing then propagates
through the division, so `PCHG` is missing for that row instead of raising
`division_by_zero`; nonzero baselines retain the original calculation.
