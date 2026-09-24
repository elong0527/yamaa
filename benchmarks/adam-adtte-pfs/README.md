# Progression-Free Survival

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-pfs.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the progression-free survival record for each
subject under the fixed `PFS` code (`Progression-Free Survival`),
setting `STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ`.

**Input:** randomization dates (`RANDDT`); tumor response
assessments for overall response (`RSTESTCD` of `OVRLRESP`) with
result (`RSSTRESC`), assessment date (`RSDTC`), and adequacy flag
(`ADEQFL` of `Y`); and disposition records with outcome
(`DSDECOD` of `DEATH`) and date (`DSDTC`). Records sharing a date
are ordered by their sequence number.

**Variables:**

- `STARTDT` is the randomization date from `RANDDT`.
- `ADT` is the event date, the earlier of the progression and death
  dates, when the subject has an event; else the last adequate
  assessment date.
- `AVAL` is the number of days from `STARTDT` through `ADT`,
  counting the randomization day as day one.
- `CNSR` is `0` when an event occurred (a progression or death date is
  present), `1` otherwise. The event always wins: a death after the last
  adequate assessment is still an event.
- `EVNTDESC` is `DISEASE PROGRESSION` for a progression event,
  `DEATH` for a death event, and `CENSORED` when both dates are
  absent. A progression and a death on the same day count as
  progression.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` trace `ADT` to its source:
  `DS` with `DSDTC` and the disposition sequence number for a
  death event, or `RS` with `RSDTC` and the response sequence
  number for a progression event or a censored assessment.

**Note:** only assessments flagged adequate can supply a
progression event or a censoring date, so an inadequate
progression assessment leaves the subject censored at the last
adequate assessment.

**Standard:** ADaM | **Domain:** ADTTE
