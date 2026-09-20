# Dose Intervals

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ex-intervals.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** build one Exposure (EX) record per constant-dose interval from
collected administration records, carrying `EXSEQ`, `EXTRT`, `EXDOSE`,
`EXDOSU`, `EXDOSFRQ`, `EXSTDTC`, `EXENDTC`, and `EXADJ`.

**Input:** collected administration rows, one per dosing day, carrying
treatment, dose, dose unit, dosing frequency, administration date, and
the adjustment reason recorded when the dose changed.

**Variables:**

- `EXSEQ` numbers the subject's intervals in the order they started, by
  start date then treatment. With `STUDYID` and `USUBJID` it identifies
  the record.
- `EXTRT` is the administered treatment name as collected.
- `EXDOSE` is the dose level of the interval as collected, including `0`
  for an interrupted interval.
- `EXDOSU` is the dose unit as collected.
- `EXDOSFRQ` is the dosing frequency as collected.
- `EXSTDTC` is the first administration date at the interval's dose
  level.
- `EXENDTC` is the last administration date at the interval's dose
  level.
- `EXADJ` is the adjustment reason recorded when the interval's dose
  level started, for example `DOSE REDUCED` or `DOSE INTERRUPTED`; blank
  when the dose was never adjusted.

**Note:** a dose interruption is its own interval with a zero dose and
the reason recorded. A dose level resumed after an interruption is not
split; the record spans the whole level.

Provenance: one record per constant dosing interval follows the CDISC
SDTMIG exposure assumption, with the adjustment reason on the record
where the new dose took effect. All subjects, dates, doses, and reasons
are invented fixtures.

**Standard:** SDTM | **Domain:** EX
