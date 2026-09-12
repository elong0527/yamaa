# ADaM ADLB: reject a viral load reported below the assay limit

This example uses collected viral-load results as they were reported to attempt
one record per subject and parameter:

- `AVAL` is the reported number of copies per millilitre.

One result was reported as being under a limit rather than as a number. The
limit it names is information, so reading it as that number, as zero, or as
absent would each replace a reported fact with a chosen one, and the
specification states none of them. The run must fail and no artifact is
accepted.

## How to fix

Retain the reported text in a string column and state what the numeric analysis
value should be. If a result such as `<50` is intentionally represented as a
missing numeric value, handle the failed conversion explicitly:

```yaml
derivation:
  value:
    source: LB.LBSTRESC
  conversion_failure: null
```

If the study uses a numeric substitution for values below the assay limit,
derive that documented value instead and keep the original character result
for traceability.
