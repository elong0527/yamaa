# Reject reference limits that name a folder

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-dataset-path-directory.html)

**Goal:** build one laboratory analysis record for every subject
and test named by the collected test code, carrying the collected
result in `AVAL` with the sex in `SEX` the limits are chosen by
and the upper limit of normal in `ANRHI` for that test and sex.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus a reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would be the sex the limits are chosen by, copied from
  the collected record.
- `AVAL` would be the collected result, copied from `LBSTRESN`.
- `ANRHI` would be the upper limit of normal for that test and
  sex, found by matching the collected test code and `SEX`
  against the reference table.

The limits are named as the folder `input/lbref` rather than a
table file. A folder holds whatever is added to it, so the limits
a run would read depend on what happens to be filed there, and two
runs of one study could disagree. The run is rejected before any
data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide which reviewed table the study was approved against. When limits arrive
as several files, combine them into one reviewed table first, then name that
table instead of the folder that holds it:

```yaml
datasets:
  LBREF:
    path: input/lbref/limits.csv
```

A study reads the table it was approved against, not a folder that grows.
