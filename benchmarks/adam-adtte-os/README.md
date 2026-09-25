# Overall Survival

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-os.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive one overall-survival record per subject with
parameter code `OS` (`Overall Survival`), carrying `STARTDT`,
`ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `CNSDTDSC`, `SRCDOM`,
`SRCVAR`, and `SRCSEQ`.

**Input:** subject-level randomization and last-known-alive dates
(`RANDDT`, `LSTALVDT`) alongside response records holding death
analysis dates (`ADT`). The death selection keeps only dated
response records where `PARAMCD` is `DEATH`, `AVALC` is `Y`, and
`ANL01FL` is `Y`; it takes the one with the earliest `ADT` (on a
tied date, the lower sequence number wins) and treats a subject
with no such record as having no death.

**Variables:**

- `STARTDT` is the randomization date (`RANDDT`), the origin from
  which survival time is counted.
- `ADT` is the analysis date: the death date when a qualifying
  death falls on or after `STARTDT`, and `STARTDT` itself when
  the death falls before it. With no death, `ADT` is the
  last-alive date when that date falls after randomization, else
  the randomization date (including a tie).
- `AVAL` is the number of days from `STARTDT` through `ADT`,
  counting both endpoints.
- `CNSR` is `0` when the subject has a qualifying death and `1`
  otherwise.
- `EVNTDESC` is `Death` when the subject has a qualifying death,
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
