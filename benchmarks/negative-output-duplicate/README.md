# Reject Duplicate Subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-output-duplicate.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record for every subject carrying the collected
age (`AGE`) and sex (`SEX`). Each subject may appear only once in
the result.

**Input:** collected demographics records carrying age (`AGE`) and
sex (`SEX`).

**Variables:**

- `AGE` would be the collected age, carried into the result
  unchanged.
- `SEX` would be the sex collected in the demographics records,
  carried into the result unchanged.

**Note:** the result is built one row per collected record, so a
subject entered twice reaches the result twice. That rejects the
run even when the two records agree; here they also disagree on the
age, and keeping either record, or merging the two, would report an
age the collected data does not support. The expected file shows
the completed rows as they reach the one-row-per-subject check, but
the repeated subject still rejects the run and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Reconcile the two source records and correct the governed demographics input
so it contains one supported age for the subject. If multiple source records
are legitimate, build the rows from a list holding each subject once and
declare an ordered record-selection rule for the demographics record; do not
rely on source order to discard one. The completed output must contain exactly
one row for each key.
