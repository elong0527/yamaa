# ADaM ADLB: reject reference limits named by a machine location

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits are named by a location on the machine that runs the study rather
than by a file the study carries. What that location holds depends on the
machine, so the study cannot be rebuilt anywhere else and cannot be reviewed
from what it contains. The run must fail and no artifact is accepted.

## How to fix

Copy the governed reference limits into the study and name them where the
study keeps its data:

```yaml
datasets:
  LBREF:
    path: input/lbref.csv
```

If several studies share one limit table, give each study the version it was
run against instead of pointing every study at one machine location. A shared
location that is edited between runs changes results that were already
reported.
