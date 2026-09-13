# Reject a log result from an undetectable value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-compute-ln-of-zero.html)

**Goal:** carry each collected viral-load result into `AVAL` and
add its natural logarithm as `AVALLN`, with one record for each
subject and parameter.

**Input:** laboratory records carrying test code (`LBTESTCD`) and
numeric result (`LBSTRESN`), including a viral-load test whose
result was reported as zero because the assay detected nothing.

**Variables:**

- `AVAL` would be the analysis value, mapped from the collected
  numeric result (`LBSTRESN`).
- `AVALLN` would be the natural logarithm of `AVAL`, which the
  analysis models rather than the untransformed result.

Zero has no logarithm, and a result below the limit of detection
needs a stated substitution before it can be transformed. The run
fails and no artifact is accepted.

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
