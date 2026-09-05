# ADaM ADLB: reject reference limits the study does not hold

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The study reads a table of limits it does not carry. Nothing else in the
study supplies an upper limit, so no result could be compared against the
range it was collected under, and a reader could not tell a table that was
forgotten from one deliberately left out. The run must fail and no artifact
is accepted.

## How to fix

Add the approved limit table to the study under the name it is read by, with
one row per test and sex:

```text
LBTESTCD,SEX,ANRHI
ALT,F,33
ALT,M,41
```

If a study genuinely has no reference limits for a test, say so explicitly by
supplying a result for the unmatched case rather than by leaving the table
out. A file that is simply absent cannot be told apart from one that was
forgotten.
