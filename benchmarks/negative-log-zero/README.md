# Reject Log Zero

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-log-zero.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each collected laboratory result into `AVAL` and
add its natural logarithm as `AVALLN`, with one record for each
subject and parameter.

**Input:** laboratory records carrying test code (`LBTESTCD`) and
numeric result (`LBSTRESN`), where a viral load the assay did not
detect is reported as zero.

**Variables:**

- `AVAL` would be the analysis value, mapped from the collected
  numeric result (`LBSTRESN`).
- `AVALLN` would be the natural logarithm of `AVAL`, which the
  analysis models rather than the untransformed result, and blank
  when `AVAL` is blank. Zero has no logarithm, and a result below the
  limit of detection needs a stated substitution before it can be
  transformed, so the run fails while the values are computed and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

State the analysis policy for an undetectable value. If it should produce a
missing transformed result, guard zero explicitly:

```yaml
derivation:
  compute:
    expr: "LN(NULLIF(AVAL, 0))"
```

If the study instead substitutes a value related to the assay limit, derive
that stated substitute first and apply `LN` to it. Do not replace zero with an
unstated constant.
