# ADaM ADLB: reject reference limits named by an unapproved location

This example uses collected laboratory results with a table of reference
limits by test and sex to attempt one record per subject and parameter:

- `SEX` is the sex the limits are chosen by;
- `AVAL` is the collected result;
- `ANRHI` is the upper limit of normal for that test and sex.

The limits are named by a location on the machine that runs the study. A run
may read such a location, because code and data are commonly kept apart, but
only one that whoever starts the run approved in advance. This study was
started with its own directory as the only approved place to read from, so
these limits name nothing the run may open. Naming a location cannot approve
it: if it could, the file under review would decide what the run reads. The
run must fail and no artifact is accepted.

## How to fix

Copy the governed reference limits into the study and name them where the
study keeps its data:

```yaml
datasets:
  LBREF:
    path: input/lbref.csv
```

Keep a shared limit table outside the study only when the runner approves the
directory that holds it as a data root, and then name it by its rooted path.
Give each study the version it was run against either way: a shared location
that is edited between runs changes results that were already reported.

[Rendered view](https://elong0527.github.io/yamaa/examples/negative-dataset-path-absolute.html)
