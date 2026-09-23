# Partial Adverse Event Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-partial-dates.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE) with the collected start
and end kept at the precision they were collected (`AESTDTC`,
`AEENDTC`), and the study day (`AESTDY`, `AEENDY`) filled only when
the date is complete.

**Input:** adverse event records carrying the reported term
(`AETERM`) and the collected start and end as separate year, month,
and day fields, where any of the three may be unknown, plus the
subject reference start date (`RFSTDTC`) from demographics, empty
when the subject has no usable reference date.

**Variables:**

- `AESTDTC` / `AEENDTC` are the collected dates written as ISO 8601
  text at the precision collected: a full date (`2026-03-15`), a
  year and month (`2026-03`), or a year alone (`2026`). Nothing is
  imputed and nothing is completed; a missing collection stays
  empty.
- `AESTDY` / `AEENDY` are the study days counted from `RFSTDTC`
  (day 1, no day zero). A study day is derived only for a complete
  date: a partial date keeps no study day even when the reference
  date is known, and a complete date keeps none when the reference
  date is unknown. A date before the reference start counts back
  from -1.

**Standard:** SDTM | **Domain:** AE
