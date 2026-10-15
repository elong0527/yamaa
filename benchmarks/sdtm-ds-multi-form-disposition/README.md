# Disposition from Several Source Forms

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ds-multi-form-disposition.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Disposition (DS) record per subject
for each of four study events carried in a single ODM
item extract: informed consent and randomization as
protocol milestones, and end of treatment and end of
study as disposition events. Each event gives its own
reported term (`DSTERM`), controlled term (`DSDECOD`),
category (`DSCAT`), subcategory (`DSSCAT`) and date
(`DSSTDTC`), with `DSSEQ` numbering each subject's
records in the order they happened.

**Input:** one ODM file (`input/odm.csv`) with one row
per collected item value, keyed by study, subject,
study event, form and item. The four study events are
`CONSENT`, `RAND`, `EOT` and `EOS`; every form carries
a date item (`IT.DS.DTC`), and the treatment and study
forms also carry a completion status (`IT.DS.COMP`), a
reported reason (`IT.DS.REASON`) and a standardized
reason (`IT.DS.REASONCD`).

**Variables:**

- `DSSEQ` numbers each subject's records in the order
  they happened, by `DSSTDTC` and then by `DSTERM`, so
  records on the same day keep a fixed order. With
  `STUDYID` and `USUBJID` it identifies the record.
- `DSTERM` holds the reported term: the controlled term
  for a milestone; `COMPLETED` when the completion
  status says so, or `SCREEN FAILURE` at end of study
  when it says so; else the collected reason.
- `DSDECOD` holds the controlled term: the same term as
  `DSTERM` for a milestone, a completion or a screen
  failure, else the collected standardized reason.
- `DSCAT` follows the event, not the term:
  `PROTOCOL MILESTONE` for consent and randomization,
  `DISPOSITION EVENT` for end of treatment and end of
  study.
- `DSSCAT` names the event: `INFORMED CONSENT`,
  `RANDOMIZATION`, `END OF TREATMENT` or `END OF STUDY`.
- `DSSTDTC` is the collected date item for that record.

**Note:** each collected form gives one record and an
event with no form gives none, so a screen failure has
only its consent and end-of-study records.

Provenance: `DSCAT`/`DSSCAT`/`DSTERM`/`DSDECOD` follow
the CDISC SDTMIG DS assumptions (protocol milestones
carry the same value in `DSTERM` and `DSDECOD`), and the
controlled terms follow the FDA Technical Conformance
Guide DS guidance. All subjects, dates and reasons are
invented fixtures.

**Standard:** SDTM | **Domain:** DS
