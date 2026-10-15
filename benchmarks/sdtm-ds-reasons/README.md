# Discontinuation Reasons

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ds-reasons.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one Disposition (DS) record per subject
for each of two milestones: end of treatment and end of
study, carrying the reported reason (`DSTERM`), the
controlled reason (`DSDECOD`), the milestone (`DSSCAT`),
the category (`DSCAT`) and the start date (`DSSTDTC`),
with `DSSEQ` numbering the subject's two milestones.

**Input:** EDC output in long form, one row per collected
item; each row names the subject, the visit (end of
treatment or end of study), the item (completion flag,
reason text, controlled code, or date) and the value, in
no particular order.

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
  completion flag says so, else the collected reason text.
- `DSDECOD` holds the controlled term: `COMPLETED` when
  completed, else the collected code, which must be
  `ADVERSE EVENT` or `OTHER`.
- `DSSTDTC` is the collected start date for that
  milestone.

**Note:** the completion flag, the reason, the code and
the date of one milestone are four separate collected
rows. One record covers all four, and each variable reads
its own item, so the order of the input rows never
reaches the output. Records are listed by subject and
then by `DSSEQ`.

Provenance: `DSCAT`/`DSSCAT`/`DSTERM`/`DSDECOD` follow
the CDISC SDTMIG DS assumptions, and the controlled terms
follow the FDA Technical Conformance Guide DS guidance.
All subjects, dates and reasons are invented fixtures.

**Standard:** SDTM | **Domain:** DS
