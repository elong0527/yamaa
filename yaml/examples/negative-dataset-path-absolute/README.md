# Reject reference limits named by an absolute location

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-dataset-path-absolute.html)

**Goal:** attempt one record per subject and parameter carrying
the collected result in `AVAL` with the upper limit of normal for
that test and sex in `ANRHI`, chosen by `SEX`.

**Input:** collected laboratory records carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus a reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would be the collected sex, taken from `SEX` in the
  collected records, and also chooses the limit.
- `AVAL` would be the collected numeric result, taken from
  `LBSTRESN`.
- `ANRHI` would be the upper limit of normal from the reference
  table for the record's test code and sex, matched on test code
  and `SEX`.

The reference table is named by an absolute location,
`/shared/reference/lbref.csv`, which names a place on the machine
that runs the study rather than a file the study carries. A run
may only open files the study holds, so the run is rejected before
any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

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
