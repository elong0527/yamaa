# Derive overall survival

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtte-overall-survival.html)

**Goal:** derive one overall-survival record per subject with
parameter code `OS` (`Overall Survival`), carrying `STARTDT`,
`ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `CNSDTDSC`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ`.

**Input:** subject-level dates (`RANDDT`, `LSTALVDT`) alongside
response records holding death analysis dates (`ADT`). The death
selection keeps response records where `PARAMCD` is `DEATH`,
`AVALC` is `Y`, and `ANL01FL` is `Y`, takes the record with the
earliest `ADT` (ties unspecified), and treats a subject with no
such record as having no death.

**Variables:**

- `STARTDT` is the randomization date (`RANDDT`), the origin from
  which survival time is counted.
- `ADT` is the analysis date: the death date when a qualifying
  death falls on or after `STARTDT`, and `STARTDT` itself when
  the death falls before it. With no death, `ADT` is the
  last-alive date when that date falls after randomization, else
  the randomization date (including a tie).
- `AVAL` is the inclusive number of days from `STARTDT` through
  `ADT`, counting both endpoints.
- `CNSR` is `0` when a qualifying death record exists and `1`
  otherwise.
- `EVNTDESC` is `Death` when a qualifying death record exists,
  `Alive` when censoring at a last-alive date after
  randomization, and `Randomization` otherwise.
- `CNSDTDSC` is blank for a death, `Alive During Study` when
  censoring at the last-alive date, and `Randomization` when
  censoring at randomization.
- `SRCDOM` is `ADRS` for a death and `ADSL` for censoring.
- `SRCVAR` is `ADT` for a death, `LSTALVDT` when censoring at the
  last-alive date, and `RANDDT` otherwise.
- `SRCSEQ` is the sequence number from the selected death record,
  and blank when the record is censored.

**Note:** a death dated before randomization still counts as an
event: `ADT` is set to randomization while `CNSR`, `EVNTDESC`,
and the source columns still reflect the death.

**Standard:** ADaM | **Domain:** ADTTE
