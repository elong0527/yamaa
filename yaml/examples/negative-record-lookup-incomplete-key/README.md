# SDTM LB: reject a reference limit chosen without a sex

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per result:

- `LBSTNRHI` is the upper limit of normal for the test and sex of the result.

One subject's sex was never collected, so no record of limits can be looked
for. Reporting the limit as absent would say the table has no entry for this
result, when the truth is that nothing was asked of it, so the run must fail
and no artifact is accepted.

## How to fix

Recover the missing sex when possible. If an incomplete lookup key is intended
to make every value read from the lookup missing, declare that policy on the
record lookup:

```yaml
record_lookups:
  - id: REFRANGE
    dataset: LBRANGE
    source: [LBTESTCD, SEX]
    key: [LBTESTCD, SEX]
    incomplete: missing
```

This is separate from `unmatched`, which handles a complete key that the table
does not contain.
