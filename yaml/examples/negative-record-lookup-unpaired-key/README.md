# SDTM LB: reject a reference limit matched against nothing

This example uses collected laboratory results with a table of reference limits
by test and sex to attempt one record per result:

- `LBSTNRHI` is the upper limit of normal for the test and sex of the result.

The record of limits is chosen by test and sex, but the specification never
says which columns of the limit table those values are matched against.
Matching them against columns of the same name would make a rule out of a
coincidence of naming, so the run must fail and no artifact is accepted.

## How to fix

Declare the lookup-table columns paired with the current-row values:

```yaml
record_lookups:
  - id: REFRANGE
    dataset: LBRANGE
    source: [LBTESTCD, SEX]
    key: [LBTESTCD, SEX]
```

`source` and `key` pair by position and must always be declared together.
