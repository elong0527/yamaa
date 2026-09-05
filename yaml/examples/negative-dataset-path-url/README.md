# ADaM ADLB: reject reference limits named by a web address

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits are named by a web address. What an address returns depends on
when it is fetched and on who fetches it, so two runs of one study can be
given different limits while both record the same request, and neither run
can say which limits it used. The run must fail and no artifact is accepted.

## How to fix

Fetch the governed limits once, record which version was received, and store
that copy with the study:

```yaml
datasets:
  LBREF:
    path: input/lbref.csv
```

Retrieval belongs to the step that assembles study data, where the received
version can be recorded and reviewed. It is not part of building the analysis
dataset.
