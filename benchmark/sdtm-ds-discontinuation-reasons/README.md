# Carry discontinuation reasons into DS

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ds-discontinuation-reasons.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** build one Disposition (DS) record per subject
for each of two milestones: end of treatment and end of
study, carrying the reported reason (`DSTERM`), the
controlled reason (`DSDECOD`), the milestone (`DSSCAT`),
the category (`DSCAT`) and the start date (`DSSTDTC`),
with `DSSEQ` numbering the subject's two milestones.

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

- `DSSEQ` numbers the subject's milestones in the order a
  subject reaches them: `1` for end of treatment and `2`
  for end of study. With `STUDYID` and `USUBJID` it
  identifies the record.
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

Note: the completion flag, the reason, the code and the
date of one milestone are four separate collected rows.
One record covers all four, and each variable names the
item it reads, so the shuffled collection order of the
input never reaches the output. Records are listed by
subject and then by `DSSEQ`. A subject stops treatment
before leaving the study, so `DSSTDTC` rises with `DSSEQ`
within a subject.

Provenance: `DSCAT`/`DSSCAT`/`DSTERM`/`DSDECOD` follow
the CDISC SDTMIG DS assumptions, and the controlled terms
follow the FDA Technical Conformance Guide DS guidance.
All subjects, dates and reasons are invented fixtures.

**Standard:** SDTM | **Domain:** DS
