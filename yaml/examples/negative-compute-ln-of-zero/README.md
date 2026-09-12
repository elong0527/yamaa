# ADaM ADLB: reject a log result from an undetectable value

This example uses collected viral-load results to attempt one record per
subject and parameter:

- `AVAL` is the collected number of copies per millilitre;
- `AVALLN` is its natural logarithm, which the analysis models rather than the
  untransformed result.

One result was reported as zero because the assay detected nothing, and zero
has no logarithm. A result below the limit of detection needs a stated
substitution before it can be transformed, so the run must fail and no artifact
is accepted.

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
