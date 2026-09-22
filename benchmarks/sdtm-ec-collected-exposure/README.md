# Collected Daily Dosing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ec-collected-exposure.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one EC (Exposure as Collected) record per dosing-log
day, carrying `ECSEQ`, `ECTRT`, `ECOCCUR`, `ECREASND`, `ECDOSE`,
`ECDOSU`, `ECDOSFRM`, `ECDOSFRQ`, `ECROUTE`, `ECSTDTC`, `ECENDTC`, and
`ECADJ`.

**Input:** a daily dosing log, one row per subject per day, carrying
the date, tablets taken, whether the dose was taken, the reason a dose
was missed, and any dose adjustment.

**Variables:**

- `ECSEQ` numbers the subject's dosing days in date order. With
  `STUDYID` and `USUBJID` it identifies the record.
- `ECOCCUR` is `Y` when the subject took the day's dose, `N` when the
  dose was not taken.
- `ECREASND` is the reason the dose was not taken, for example
  `SUBJECT FORGOT DOSE`; blank when the dose was taken.
- `ECDOSE` is the tablets taken that day; blank on a day the dose was
  not taken.
- `ECSTDTC` and `ECENDTC` are the dosing day, since one record covers
  one day.
- `ECADJ` is the adjustment reason in effect on the dosing day, for
  example `DOSE REDUCED`; blank when the dose was never adjusted.

**Note:** a dose reduced after an adverse event carries the adjustment
reason (`DOSE REDUCED`), not the event. A missed day keeps the
treatment's unit, form, frequency, and route with no dose and the
missed reason.

**Standard:** SDTM | **Domain:** EC
