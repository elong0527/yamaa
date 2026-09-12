# ADaM ADLB: reject reference limits reached through a stand-in name

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The name the study reads is a stand-in that points at another file. Where it
points can be changed without changing the study, so the limits that were
reviewed and the limits that are read need not be the same, and nothing in
the study records the difference. The run must fail and no artifact is
accepted.

## How to fix

Read the limits under the name that holds them:

```yaml
datasets:
  LBREF:
    path: input/reference/lbref.csv
```

Storing the file itself where the study reads it works equally well. Either
way one name reaches one file, and reviewing the study is reviewing what it
reads.
