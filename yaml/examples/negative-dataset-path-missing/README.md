# Reject a run whose reference table is missing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-dataset-path-missing.html)

**Goal:** build one analysis row for each collected laboratory
result, carrying `SEX` and `AVAL` through and adding `ANRHI` from
a reference table of limits by test and sex.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus the reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would contain the subject's sex, carried from `SEX` in
  the laboratory results.
- `AVAL` would contain the collected numeric result, carried from
  `LBSTRESN`.
- `ANRHI` would be the upper limit of normal taken from the
  reference row whose test code matches the collected test code
  and whose sex matches the subject's sex.

The reference table is absent from the study, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Decide first whether the study should carry reference limits: add the approved
table when the limits exist, and state an explicit policy only when the study
genuinely has none.

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
