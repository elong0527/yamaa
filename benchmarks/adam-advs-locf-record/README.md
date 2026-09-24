# Carry one observed record to each planned visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-locf-record.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** give each planned visit the result, date, and sequence number
(`AVAL`, `ADT`, `QSSEQ`) of one observation carried forward to it.

**Input:** the planned analysis visits per subject and parameter, and
the collected vital signs observations, each flagged as selected for
analysis or not.

**Variables:**

- `AVISITN` identifies the planned analysis visit.
- `AVAL` is the latest non-missing result selected for analysis at or
  before that visit, within the same subject and parameter. With no such
  result it is missing.
- `ADT` and `QSSEQ` are the date and sequence number of that same
  observation. A missing date stays missing even when an earlier
  observation has a date.

**Note:** the latest observation is the one with the highest visit
number, then the highest sequence number within that visit.

**Standard:** ADaM | **Domain:** ADVS
