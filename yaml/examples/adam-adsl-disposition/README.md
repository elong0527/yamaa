# Summarize each subject's final disposition

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-disposition.html)

**Goal:** give each subject a single end-of-study summary holding
`EOSDT`, `EOSDECOD`, `EOSREAS`, `EOSSTT`, and `DCSREAS`.

**Input:** demographics rows identifying each subject, plus
disposition records (`DS`) with category (`DSCAT`), coded term
(`DSDECOD`), reported term (`DSTERM`), start date (`DSSTDTC`), and
sequence number (`DSSEQ`).

**Variables:**

- `EOSDT` is the latest start date (`DSSTDTC`) among the subject's
  records whose category (`DSCAT`) is `DISPOSITION EVENT`; a record
  without a date never counts, and a subject with no dated
  disposition record is empty.
- `EOSDECOD` is the coded term (`DSDECOD`) on the last dated
  disposition event record whose category is `DISPOSITION EVENT`,
  ordered by start date (`DSSTDTC`) and then by sequence number
  (`DSSEQ`); empty when the subject has no dated disposition
  record.
- `EOSREAS` is the reported term (`DSTERM`) on that same last
  record; empty when the subject has no dated disposition record.
- `EOSSTT` is `COMPLETED` when the last coded term is `COMPLETED`,
  `DISCONTINUED` for any other recorded term, and `ONGOING` when the
  subject has no dated disposition record.
- `DCSREAS` repeats the end-of-study reason (`EOSREAS`) for a
  subject whose status is `DISCONTINUED`; empty otherwise.

**Note:** date, term, and reason come from a single last event, so
the three always describe the same event: when two events share the
latest date, the higher sequence number (`DSSEQ`) decides.

**Standard:** ADaM | **Domain:** ADSL
