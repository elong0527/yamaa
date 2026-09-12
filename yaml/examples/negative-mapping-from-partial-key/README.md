# ADaM ADLB: reject a reference range chosen without a sex

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

One subject's sex was never collected, so the pair that chooses a limit is
incomplete and no entry can be looked for. Falling back to one sex, or to a
combined limit, would answer with a range the study never stated, so the run
must fail and no artifact is accepted. An uncollected sex is a different
condition from a sex the limit table does not cover, and a specification may
answer them differently.

## How to fix

Recover and correct the missing sex when possible. If the intended result is a
missing reference limit whenever any lookup input is missing, state that with
the mapping operation's `missing` handler:

```yaml
mapping_from:
  source: [PARAMCD, SEX]
  dataset: LBREF
  key: [LBTESTCD, SEX]
  value: ANRHI
  missing: null
```

This does not handle a complete key that is absent from `LBREF`; that separate
condition uses `unmapped`.
