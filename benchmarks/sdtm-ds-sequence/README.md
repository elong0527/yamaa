# Disposition Sequence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ds-sequence.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one output record per collected disposition
record, carrying `DSDECOD`, `DSCAT` and `DSDTC`, with `DSSEQ`
numbering each subject's records from the earliest collection
date.

**Input:** one row per collected form, with the subject in the
subject field, the recorded outcome in the outcome field, and the
collection date text in the date field. The form label does not
feed any output column.

**Variables:**

- `DSSEQ` numbers the subject's records from the earliest
  collection date as entered, not as completed: a partial entry
  such as `2024-01` comes before every full date in that month,
  a year alone before every full date in that year, and a
  record with no date at all is numbered after dated records.
- `DSDECOD` copies the recorded outcome: `COMPLETED`, `RANDOMIZED`,
  `ADVERSE EVENT` or `SCREEN FAILURE`.
- `DSCAT` is `PROTOCOL MILESTONE` when the outcome is `RANDOMIZED`,
  and `DISPOSITION EVENT` for any other outcome.
- `DSDTC` is the collection date; a partial entry such as
  `2024-01` is completed to the 15th of that month, and a year
  alone is completed to June 15th. It is empty when the date is
  missing or cannot be read.

**Standard:** SDTM | **Domain:** DS
