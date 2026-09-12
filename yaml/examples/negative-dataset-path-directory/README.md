# ADaM ADLB: reject reference limits that name a folder

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits name a folder rather than a table. A folder holds whatever is
added to it, so the limits a run receives would depend on what happens to be
filed there, and two runs of one study could disagree without either being
changed. The run must fail and no artifact is accepted.

## How to fix

Name the limit table inside the folder:

```yaml
datasets:
  LBREF:
    path: input/lbref/limits.csv
```

When limits arrive as several files, combine them into one reviewed table
first. A study reads the table it was approved against, not a folder that
grows.
