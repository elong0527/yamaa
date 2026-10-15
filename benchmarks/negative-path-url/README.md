# Reject URL Path

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-path-url.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attempt one record per subject and laboratory test
carrying the collected result in `AVAL` and the upper limit of
normal for that test and the subject's sex in `ANRHI`.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
plus a reference table of upper limits by test code and sex.

**Variables:**

- `SEX` would be the collected sex, taken from `SEX` in the
  collected records, and also chooses the limit.
- `AVAL` would be the collected numeric result, taken from
  `LBSTRESN`.
- `ANRHI` would be the upper limit of normal from the reference
  table for the record's test code and sex; missing when the table
  has no limit for that pair or the sex is missing.

**Note:** the reference table is named by a web address,
`https://reference.example.org/limits/lbref.csv`, whose contents
depend on when it is fetched and on who fetches it, so two runs of
one study could read different limits while recording the same
request. A run opens only stored files, never a web address, so
the run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Fetch the governed limits once, record which version was received, and store
that copy with the study:

```yaml
input:
  LBREF:
    path: input/lbref.csv
```

Retrieval belongs to the step that assembles study data, where the received
version can be recorded and reviewed. It is not part of building the analysis
dataset.
