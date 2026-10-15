# Select the Actual Treatment and Its Exposure Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-treatment.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** select each subject's actual treatment (`TRT01A`), the first and
last exposure dates and times (`TRTSDT`, `TRTSDTM`, `TRTEDT`, `TRTEDTM`),
the imputation flags (`TRTSTMF`, `TRTETMF`), the treatment duration in days
(`TRTDURD`), and the safety population flag (`SAFFL`).

**Input:** one demographics (DM) record per subject carrying the
actual arm (`ACTARM`), plus exposure (EX) records carrying the
treatment name (`EXTRT`), sequence number (`EXSEQ`), start date
(`EXSTDTC`), and end date (`EXENDTC`). Records are built from the
demographics records, so an exposure record with no matching
demographics record adds no record.

**Variables:**

- `TRT01A`: treatment actually received, uppercased: the treatment of the
  earliest qualifying exposure (a qualifying record names `VITAMIN D3`
  or `PLACEBO` and carries a start), with ties on the start broken by the
  lower `EXSEQ`; then the actual arm (`ACTARM`) when no exposure
  qualifies; then the text `NOT TREATED` when neither source names a
  treatment.
- `TRTSDT` and `TRTSDTM`: date and date/time of the earliest qualifying
  exposure start; a start collected without a time uses `00:00:00`. Both are
  blank when the subject has no qualifying exposure record.
- `TRTSTMF`: `H` when the first exposure start was collected without a time,
  so its time was imputed; otherwise blank.
- `TRTEDT` and `TRTEDTM`: date and date/time of the latest exposure end
  among `VITAMIN D3` and `PLACEBO` records (a record need not have a
  start to name an end); an end collected without a time uses `23:59:59`.
  Both are blank when no such record has an end.
- `TRTETMF`: `H` when the last exposure end was collected without a time,
  so its time was imputed; otherwise blank.
- `TRTDURD`: number of days from `TRTSDT` to `TRTEDT`, counting
  both the first and the last day (inclusive), so a single
  treatment day gives one; blank when either date is blank.
- `SAFFL`: `Y` when the subject has a treatment start date, `N`
  otherwise (never blank).

**Note:** the exposure dates, date/times, and duration are all present or
all blank together; the latest end can come from a record whose start was
never collected.

**Standard:** ADaM | **Domain:** ADSL
