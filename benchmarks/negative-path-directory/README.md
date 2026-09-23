# Reject Folder Path

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-path-directory.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt one record per subject and laboratory test
carrying the collected result in `AVAL` and the upper limit of
normal for that test and the subject's sex in `ANRHI`.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus a reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would be the sex the limits are chosen by, copied from
  the collected record.
- `AVAL` would be the collected result, copied from `LBSTRESN`.
- `ANRHI` would be the upper limit of normal for that test and
  sex, found by matching the collected test code and `SEX`
  against the reference table; missing when the table has no
  limit for that pair or the sex is missing.

**Note:** the limits are named as the folder `input/lbref` rather
than a table file. A folder holds whatever is added to it, so the
limits a run would read depend on what happens to be filed there,
and two runs of one study could disagree. The run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide which reviewed table the study was approved against. When limits arrive
as several files, combine them into one reviewed table first, then name that
table instead of the folder that holds it:

```yaml
input:
  LBREF:
    path: input/lbref/limits.csv
```

A study reads the table it was approved against, not a folder that grows.
