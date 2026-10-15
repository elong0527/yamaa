# Derive Date Epoch

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-derive-date-epoch.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the per-record `derive` step of an aggregation:
convert each record's date to an integer day count with `to_epoch_day`,
then summarize the integers.

**Input:** exposure records with sequence numbers (`EXSEQ`) and
start dates (`EXSTDT`). Only the record with sequence number 1 becomes
an output row.

**Variables:**

- `EXSEQ`: the exposure sequence number, carried through.
- `EXSTDY`: the exposure start date as a count of days since
  1970-01-01 (zero on that day, negative before it), blank when the
  start date is missing. It is not a day counted from a reference start.

**Note:** each matching exposure record's date is turned into its day
count first, so the summary works on plain integers and never on dates.
The summary then keeps the value of the one record with the row's
subject and sequence number (`ONLY`); a second matching record would
stop the run.

**Standard:** ADaM | **Domain:** ADEX
