# Order disposition records by collection date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ds-disposition-sequence.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/yaml/examples/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

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
  collection date text; an entry carrying only year and month
  is completed to the 15th before numbering, and a record with
  no date at all is numbered after dated records.
- `DSDECOD` copies the recorded outcome: `COMPLETED`, `RANDOMIZED`,
  `ADVERSE EVENT` or `SCREEN FAILURE`.
- `DSCAT` is `PROTOCOL MILESTONE` when the outcome is `RANDOMIZED`,
  and `DISPOSITION EVENT` for any other outcome.
- `DSDTC` is the collection date; a partial entry such as
  `2024-01` is completed to the 15th of that month, and a year
  alone is completed to June 15th.

**Standard:** SDTM | **Domain:** DS
