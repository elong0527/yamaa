# Partial Adverse Event Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-partial-dates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE) with the collected start
and end kept at the precision they were collected (`AESTDTC`,
`AEENDTC`), and the study day (`AESTDY`, `AEENDY`) filled only when
the date is complete.

**Input:** long-form Operational Data Model (ODM) data
(`input/odm.csv`) with one row per collected item, carrying the
collection form (`FormOID`), the form repeat (`FormRepeatKey`, the AE
number within the subject), the item (`ItemOID`), and the stored value
(`Value`). Each adverse event is one `FO.AE` form instance holding the
reported term (`IT.AE.AETERM`) and the collected start and end as
separate year, month, and day items (`IT.AE.AESTYR` / `IT.AE.AESTMO` /
`IT.AE.AESTDY`, `IT.AE.AEENYR` / `IT.AE.AEENMO` / `IT.AE.AEENDY`); a
part that was not collected has no row. Demographics (`input/dm.csv`)
supplies the subject reference start date (`RFSTDTC`), empty when the
subject has no usable reference date.

**Variables:**

- `AESTDTC` / `AEENDTC` are the collected dates written as ISO 8601
  text at the precision collected: a full date (`2026-03-15`), a
  year and month (`2026-03`), or a year alone (`2026`). Nothing is
  imputed, and a date with no part collected stays empty. A day
  collected without its month, or a month without its year, stops
  the run.
- `AESTDY` / `AEENDY` are the study days counted from `RFSTDTC`: day
  1 is the reference start date, the day before it is -1, and there
  is no day zero. Only a complete date gets a study day: a partial
  date has none even when the reference date is known, and no date
  has one when the reference date is unknown.

**Standard:** SDTM | **Domain:** AE
