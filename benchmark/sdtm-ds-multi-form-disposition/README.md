# Combine milestones and disposition events from four sources

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ds-multi-form-disposition.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit.

**Goal:** build one Disposition (DS) record per milestone or event:
consent, randomization, screening outcome, end of treatment, and
end of study, carrying `DSCAT`, `DSSCAT`, `DSTERM`, `DSDECOD`,
`DSSTDTC`, and `DSSEQ`.

**Input:** a consent form with the consent date and screening
outcome, a randomization list with the randomization date, and
end-of-treatment and end-of-study forms with the completion status,
the reason, and the event date.

**Variables:**

- `DSCAT` is `PROTOCOL MILESTONE` for consent and randomization
  records and `DISPOSITION EVENT` for screening, treatment, and
  study records; the label follows the source, not the wording.
- `DSSCAT` names the phase the event belongs to: `STUDY TREATMENT`
  for end of treatment, `STUDY` for end of study, and `SCREENING`
  for screen failure; milestones carry no subcategory, so it stays
  blank there.
- `DSTERM` is the reported wording: `INFORMED CONSENT OBTAINED`
  and `RANDOMIZED` for milestones, `COMPLETED` when a form reports
  a completion, the collected reason when treatment stops early,
  and the screening reason for a screen failure. A subject who
  finishes both phases carries `COMPLETED` twice; one who stops
  treatment early still carries `COMPLETED` for the study; a screen
  failure carries only consent and screening records.
- `DSDECOD` is the standard wording: it repeats `DSTERM` for
  milestones and completions, carries the coded reason
  (`ADVERSE EVENT`) when treatment stops early, and carries
  `SCREEN FAILURE` for a screen failure.
- `DSSTDTC` is the date of the milestone or event: the consent,
  randomization, screening outcome, or form date.
- `DSSEQ` numbers each subject's records from the earliest DSSTDTC;
  ties break by DSCAT then DSDECOD, all ascending; consent and
  randomization on the same day put consent first because both are
  PROTOCOL MILESTONE and INFORMED CONSENT OBTAINED sorts before
  RANDOMIZED.

**Note:** a `RANDOMIZED` record comes only from the randomization
list, which alone decides who was randomized: a screen failure never
gains one, and no subject gains two. Wording and categories follow
the CDISC SDTMIG disposition assumptions; all subjects and dates are
invented.

**Standard:** SDTM | **Domain:** DS
