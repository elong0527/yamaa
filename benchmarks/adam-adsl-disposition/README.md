# Summarize End-of-Study and End-of-Treatment Disposition

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-disposition.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** give each subject a single end-of-study summary holding
`EOSDT`, `EOSDECOD`, `EOSREAS`, `EOSSTT`, and `DCSREAS`, plus an
independent end-of-treatment summary holding `EOTDT`, `EOTSTT`,
`DCTREAS`, and `DCSREASP`.

**Input:** demographics rows identifying each subject, plus
disposition records (`DS`) with category (`DSCAT`), epoch
(`EPOCH`), coded term (`DSDECOD`), reported term (`DSTERM`), start
date (`DSSTDTC`), and sequence number (`DSSEQ`). Records with
`EPOCH = 'TREATMENT'` describe treatment disposition; the study
summary reads every other epoch.

**Variables:**

- `EOSDT` is the latest start date (`DSSTDTC`) among the subject's
  records whose category (`DSCAT`) is `DISPOSITION EVENT` and whose
  epoch is not `TREATMENT`; a record without a date never counts,
  and a subject with no dated disposition record is empty.
- `EOSDECOD` is the coded term (`DSDECOD`) on the subject's last
  dated study disposition record, the one with the latest start
  date; empty when the subject has no dated disposition record.
  The coded term names the specific reason (for example
  `WITHDRAWAL BY SUBJECT`), never a generic status.
- `EOSREAS` is the reported term (`DSTERM`) on that same last
  record; empty when the subject has no dated disposition record.
- `EOSSTT` is `COMPLETED` when the last coded term is `COMPLETED`,
  `DISCONTINUED` for any other recorded term, and `ONGOING` when the
  subject has no dated disposition record.
- `DCSREAS` repeats the end-of-study reason (`EOSREAS`) for a
  subject whose status is `DISCONTINUED`; empty otherwise.
- `EOTDT` is the latest start date among the subject's dated
  treatment disposition records (`EPOCH = 'TREATMENT'`); empty when
  the subject has none.
- `EOTSTT` is `COMPLETED` when the last treatment coded term is
  `COMPLETED`, `DISCONTINUED` for any other recorded term, and
  `ONGOING` when the subject has no dated treatment record.
- `DCTREAS` is the reported term on the last treatment record for a
  subject whose treatment status is `DISCONTINUED`; empty otherwise.
- `DCSREASP` carries that reported term again when the coded
  treatment term is `OTHER` — the specify text; empty otherwise.

**Note:** date, term, and reason come from a single last event, so
the three always describe the same event: when two events share the
latest date, the higher sequence number (`DSSEQ`) decides. Records
in other categories, such as protocol milestones, and records
without a date never count, so a subject whose disposition record
has no date is `ONGOING`. The study and treatment tracks are
independent: a subject who stops treatment but completes the study
is `DISCONTINUED` for treatment and `COMPLETED` for study.

**Standard:** ADaM | **Domain:** ADSL
