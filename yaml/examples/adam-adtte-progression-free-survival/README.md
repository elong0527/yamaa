# Progression-free survival

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtte-progression-free-survival.html)

**Goal:** build the progression-free survival record for each
subject under the fixed `PFS` code (`Progression-Free Survival`),
setting `STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ`.

**Input:** randomization dates (`RANDDT`); tumor response
assessments for overall response (`RSTESTCD` of `OVRLRESP`) with
result (`RSSTRESC`), assessment date (`RSDTC`), and adequacy flag
(`ADEQFL` of `Y`); and disposition records with outcome
(`DSDECOD` of `DEATH`) and date (`DSDTC`). Same-date response
records order by sequence number, and same-date disposition
records by sequence number.

**Variables:**

- `STARTDT` is the randomization date from `RANDDT`.
- `ADT` is the earlier of the first adequate progression date
  (earliest `RSDTC` with `RSSTRESC` of `PD`) and the death date
  (earliest `DSDTC`); when both are absent it is the last
  adequate assessment date. A progression and a death on the same
  day count as progression.
- `AVAL` is the number of days from `STARTDT` through `ADT`,
  counting the randomization day as day one.
- `CNSR` is `0` when a progression or death date is present and
  `1` when both are absent.
- `EVNTDESC` is `DISEASE PROGRESSION` for a progression event,
  `DEATH` for a death event, and `CENSORED` when both dates are
  absent.
- `SRCDOM`, `SRCVAR`, and `SRCSEQ` trace `ADT` to its source:
  `DS` with `DSDTC` and the disposition sequence number for a
  death event, or `RS` with `RSDTC` and the response sequence
  number for a progression event or a censored assessment.

**Note:** only assessments flagged adequate can supply a
progression event or a censoring date, so an inadequate
progression assessment leaves the subject censored at the last
adequate assessment.

**Standard:** ADaM | **Domain:** ADTTE
