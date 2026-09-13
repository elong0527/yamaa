# Reject a fractional pulse rate that cannot become a whole number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-conversion-non-integral.html)

**Goal:** derive `AVAL` from the collected pulse rate.

**Input:** collected vital signs (VS) records carrying `VSTESTCD`
and `VSSTRESN`.

**Variables:**

- `AVAL` would be the analysis value taken directly from
  `VSSTRESN`; a fractional collected value cannot take a
  whole-number value, so the run fails and no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADVS

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
