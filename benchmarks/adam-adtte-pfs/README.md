# Progression-Free Survival

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-pfs.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the progression-free survival record for each
subject under the fixed `PFS` code (`Progression-Free Survival`),
setting `STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ`.

**Input:** randomization dates (`RANDDT`); tumor response
assessments for overall response (`RSTESTCD` of `OVRLRESP`) with
result (`RSSTRESC`), assessment date (`RSDTC`), and adequacy flag
(`ADEQFL` of `Y`); disposition records with outcome
(`DSDECOD` of `DEATH`) and date (`DSDTC`); and the start date of
new anti-cancer therapy (`NTXSTDT`) for subjects who began one.
Records sharing a date are ordered by their sequence number.

**Variables:**

- `STARTDT` is the randomization date from `RANDDT`.
- `ADT` is the event date, the earlier of the progression and death
  dates, when the subject has an event; else the last adequate
  assessment date, or the randomization date when no adequate
  assessment can supply a censoring date.
- `AVAL` is the number of days from `STARTDT` through `ADT`,
  counting the randomization day as day one.
- `CNSR` is `0` when an event occurred (a progression or death date is
  present), `1` otherwise. The event always wins: a death after the last
  adequate assessment is still an event, unless new anti-cancer therapy
  started first.
- `EVNTDESC` is `DISEASE PROGRESSION` for a progression event,
  `DEATH` for a death event, and `CENSORED` when both dates are
  absent. A progression and a death on the same day count as
  progression.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` trace `ADT` to its source:
  `DS` with `DSDTC` and the disposition sequence number for a
  death event, or `RS` with `RSDTC` and the response sequence
  number for a progression event or a censored assessment. All three
  are empty when the subject is censored at randomization.

**Note:** only assessments flagged adequate can supply a
progression event or a censoring date, so an inadequate
progression assessment leaves the subject censored at the last
adequate dated assessment. An assessment without a date cannot
be the last dated assessment. When new anti-cancer therapy started
(`NTXSTDT` present), only dates on or before therapy start are
usable: a progression or death dated after therapy start is not an
event, and the subject is censored at the last adequate assessment
dated on or before therapy start. A subject with no usable adequate
assessment at all is censored at randomization, for a 1-day PFS.

**Standard:** ADaM | **Domain:** ADTTE
