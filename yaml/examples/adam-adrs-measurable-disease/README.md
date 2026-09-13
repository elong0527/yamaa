# Derive measurable disease at baseline

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adrs-measurable-disease.html)

**Goal:** derive `AVALC` and `AVAL` to flag measurable disease at
baseline for each subject.

**Input:** one record per subject from the subject-level analysis
dataset (ADSL), plus tumor identification (TU) records carrying
`TUTESTCD` (tumor test code), `VISIT`, `TUSTRESC` (standardized
result), and `TUSEQ` (sequence number). `PARAMCD` is fixed to `MDIS`
and `PARAM` to `Measurable Disease at Baseline`.

**Variables:**

- `AVALC` is `Y` when the subject has at least one TU record with
  a tumor identification test (`TUTESTCD` of `TUMIDENT`) at the
  screening visit (`VISIT` of `SCREENING`) showing target disease
  (`TUSTRESC` of `TARGET`); otherwise `N`.
- `AVAL` is `1` when `AVALC` is `Y` and `0` when `AVALC` is `N`.

**Note:** each subject in the subject-level dataset gets a flag
record, even with no matching TU record, so a subject never assessed
at screening is `N` / `0`. A TU record for a subject outside the
subject-level dataset contributes to no flag and creates none.

**Standard:** ADaM | **Domain:** ADRS
