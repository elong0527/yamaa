# Reject a subject recorded twice

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-output-duplicate-subject.html)

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

**Note:** one subject was entered twice with different ages, so the
two records disagree on the age the result should carry. Keeping
either record, or merging the two, would report an age the
collected data does not support. The expected file records the
completed rows presented to that check, but the repetition still
rejects the run and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Reconcile the two source records and correct the governed demographics input
so it contains one supported age for the subject. If multiple source records
are legitimate, use a unique subject inventory as the row driver and declare an
ordered record-selection rule for the demographics record; do not rely on
source order to discard one. The completed output must contain exactly one row
for each key.
