# Reject Fractional Pulse

**Goal:** derive `AVAL` from the collected pulse rate.

**Input:** collected vital signs (VS) records carrying `VSTESTCD`
and `VSSTRESN`; one record carries a fractional pulse rate.

**Variables:**

- `AVAL` would be the collected result from `VSSTRESN`, held as a
  whole number. A fractional result has no whole-number form unless a
  rounding rule is stated, so the run fails and no artifact is accepted.

## How to fix

If fractional pulse rates are valid for the analysis, preserve the collected
precision by declaring the result as `float`:

```yaml
- name: AVAL
  type: float
  derivation: VS.VSSTRESN
```

If the result must be an integer, the specification must choose an explicit
rule, such as `round_half_away_from_zero` with `digits: 0`, or `FLOOR`,
`CEIL`, or `TRUNC` in the derivation's expression; conversion will not choose
a rounding rule implicitly.
