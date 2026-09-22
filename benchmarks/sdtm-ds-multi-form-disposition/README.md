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
reason (`IT.DS.REASONCD`). The three subjects are
invented: 001 is consented and randomized on the same
day and completes treatment and the study; 002 stops
treatment for `Severe nausea` and then leaves the study
after moving away; 003 consents but is a screen failure
and never reaches randomization.

**Variables:**

- `DSSEQ` numbers each subject's records in the order
  they happened, by `DSSTDTC` and then by `DSTERM` so
  the same-day consent and randomization keep a fixed
  order. With `STUDYID` and `USUBJID` it identifies the
  record.
- `DSCAT` follows the event, not the term:
  `PROTOCOL MILESTONE` for consent and randomization,
  `DISPOSITION EVENT` for end of treatment and end of
  study.
- `DSSCAT` names the event: `INFORMED CONSENT`,
  `RANDOMIZATION`, `END OF TREATMENT` or `END OF STUDY`.
- `DSTERM` holds the reported term: the controlled term
  for a milestone, `COMPLETED` when the completion
  status says so, else the collected reason item
  (`Severe nausea`, `Subject moved`) or `SCREEN FAILURE`.
- `DSDECOD` holds the controlled term: the same term as
  `DSTERM` for milestones, `COMPLETED` when completed,
  else the collected standardized reason
  (`ADVERSE EVENT`, `LOST TO FOLLOW-UP`,
  `SCREEN FAILURE`).
- `DSSTDTC` is the collected date item for that record.

Note: one subject has records on the same day from two
events; the term order keeps `DSSEQ` deterministic. A
screen failure has a consent record and a disposition
record only, and never reaches randomization.

Provenance: `DSCAT`/`DSSCAT`/`DSTERM`/`DSDECOD` follow
the CDISC SDTMIG DS assumptions (protocol milestones
carry the same value in `DSTERM` and `DSDECOD`), and the
controlled terms follow the FDA Technical Conformance
Guide DS guidance. All subjects, dates and reasons are
invented fixtures.

**Standard:** SDTM | **Domain:** DS
