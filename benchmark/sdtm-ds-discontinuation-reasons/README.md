# Carry discontinuation reasons into DS

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ds-discontinuation-reasons.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit.

**Goal:** build one Disposition (DS) record per subject
for each of two milestones: end of treatment and end of
study, carrying the reported reason (`DSTERM`), the
controlled reason (`DSDECOD`), the milestone (`DSSCAT`),
the category (`DSCAT`) and the start date (`DSSTDTC`),
with `DSSEQ` numbering each subject's records from the
earliest date.

**Input:** EDC output in long form, one row per collected
item; each row names the subject, the visit (end of
treatment or end of study), the item (completion flag,
reason text, controlled code, or date) and the value. The
three subjects are invented: 001 completes treatment and
study; 002 stops treatment for `Severe nausea` and then
completes the study; 003 completes treatment and then
withdraws from the study for `Moving abroad`. Input rows
are shuffled and are not in date order.

**Variables:**

- `DSCAT` is always `DISPOSITION EVENT`.
- `DSSCAT` names the milestone: `STUDY TREATMENT` for the
  end-of-treatment form and `STUDY` for the end-of-study
  form.
- `DSTERM` holds the reported term: `COMPLETED` when the
  completion flag says so, else the collected reason text
  (`Severe nausea`, `Moving abroad`).
- `DSDECOD` holds the controlled term: `COMPLETED` when
  completed, else the collected code (`ADVERSE EVENT`,
  `OTHER`).
- `DSSTDTC` is the collected start date for that
  milestone.
- `DSSEQ` numbers the subject's records from the earliest
  `DSSTDTC`.

Records are listed by subject and then by `DSSEQ`, so the
earliest milestone of each subject comes first whatever
order the items were collected in.

Provenance: `DSCAT`/`DSSCAT`/`DSTERM`/`DSDECOD` follow
the CDISC SDTMIG DS assumptions, and the controlled terms
follow the FDA Technical Conformance Guide DS guidance.
All subjects, dates and reasons are invented fixtures.

**Standard:** SDTM | **Domain:** DS
