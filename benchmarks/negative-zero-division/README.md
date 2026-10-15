# Reject Zero Division

**Goal:** work out `PCHG`, the percent change from baseline, from
the collected result in `AVAL` and its baseline in `BASE`.

**Input:** laboratory results with baseline values from LB,
carrying the collected numeric result (`LBSTRESN`) and the
baseline it is compared with (`LBBLRESN`).

**Variables:**

- `AVAL` would be the collected numeric result, taken from
  `LBSTRESN`.
- `BASE` would be the baseline it is compared with, taken from
  `LBBLRESN`.
- `PCHG` would be the change from baseline as a percentage of
  it, worked out as `100 * (AVAL - BASE) / BASE`; missing when
  either value is missing.

**Note:** a zero baseline leaves the percentage with no value.
Leaving it missing would assume a rule the request never stated, so
the run fails and no artifact is accepted.

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
