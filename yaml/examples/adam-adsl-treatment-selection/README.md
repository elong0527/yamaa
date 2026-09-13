# Select actual treatment and its duration

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-treatment-selection.html)

**Goal:** select each subject's actual treatment (`TRT01A`),
first and last exposure dates (`TRTSDT`, `TRTEDT`), treatment
duration (`TRTDURD`), and safety population flag (`SAFFL`).

**Input:** one demographics (DM) record per subject carrying the
actual arm (`ACTARM`), plus exposure (EX) records carrying the
treatment name (`EXTRT`), sequence number (`EXSEQ`), start date
(`EXSTDTC`), and end date (`EXENDTC`). Records are built from the
demographics records, so an exposure record with no matching
demographics record adds no record.

**Variables:**

- `TRT01A`: treatment actually received, uppercased: the earliest
  qualifying exposure treatment first, where a qualifying record
  has `EXTRT` of `VITAMIN D3` or `PLACEBO` with a non-missing
  start date, ordered by `EXSTDTC` with ties broken by the lower
  `EXSEQ`; then the actual arm (`ACTARM`) when there is no
  qualifying exposure; then the text `NOT TREATED` when neither
  source names a treatment.
- `TRTSDT`: earliest qualifying exposure start date, the minimum
  of `EXSTDTC` over qualifying records; blank when the subject
  has no qualifying exposure record.
- `TRTEDT`: latest qualifying exposure end date, the maximum of
  `EXENDTC` over exposure records with `EXTRT` of `VITAMIN D3` or
  `PLACEBO` and a non-missing end date; blank when the subject
  has no such record.
- `TRTDURD`: number of days from `TRTSDT` to `TRTEDT`, counting
  both the first and the last day (inclusive), so a single
  treatment day gives one; blank when either date is blank.
- `SAFFL`: `Y` when the subject has a treatment start date, `N`
  otherwise (never blank).

**Note:** the exposure dates and the duration are all present or
all blank together; a subject with no qualifying exposure record
leaves all three blank.

**Standard:** ADaM | **Domain:** ADSL
