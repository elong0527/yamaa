# ADaM ADLB: reject reference limits stored above the study

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits are read from a folder above the study. A file outside the study
is not versioned, reviewed, or archived with it, and whoever reruns the study
later receives whatever that folder holds at the time, so the run must fail
and no artifact is accepted.

## How to fix

Bring the governed limits into the study and read them from there:

```yaml
datasets:
  LBREF:
    path: input/lbref.csv
```

If the limits are maintained centrally, copy the approved version into the
study when it is approved. A study that reaches above itself for data cannot
state which version produced its results.
