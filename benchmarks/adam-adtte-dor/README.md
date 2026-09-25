# Duration of Response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adtte-dor.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build duration of response (`DOR`) records labeled
`Duration of Response`, one for each responder, carrying
`STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `CNSDTDSC`,
`SRCDOM`, `SRCVAR` and `SRCSEQ`.

**Input:** a subject-level table holding the response date
(`RESPDT`) and the start date of a new anti-cancer therapy
(`NACTDT`); response assessments holding the assessment date
(`ADT`), the response (`AVALC`) and the record sequence number
(`ASEQ`); and a disposition table holding the disposition term
(`DSDECOD`), its date (`DSSTDTC`) and sequence number (`DSSEQ`).
Only subjects whose `RESPDT` is not blank contribute a record.

**Variables:**

- `STARTDT` is the response date, copied from the subject-level
  table.
- `ADT` is the analysis date: the event date when a counted
  event exists, else the earlier of the last valid tumour
  assessment date and the therapy start date. The progression
  date is the earliest assessment date with a progressive
  disease (`PD`) response; the death date is the earliest
  disposition date with a `DEATH` term. The earlier of the two
  is the event date, unless it falls after the therapy start
  date; an event on the therapy-start day still counts. The
  last valid tumour assessment date is the latest assessment
  date carrying a response other than not evaluable (`NE`).
  An assessment with a blank date never counts as the
  progression date or the last valid assessment. When several
  records share the chosen date, the lowest sequence number
  is the progression record and the highest is the last valid
  assessment record.
- `AVAL` is the number of days from `STARTDT` through `ADT`,
  counting both endpoints; it is at least `1`.
- `CNSR` is `0` when a counted event exists and `1` when the
  record is censored.
- `EVNTDESC` names the outcome: `CENSORED` when no event
  counts; `DISEASE PROGRESSION` when the event date equals the
  progression date (so a same-day progression and death is
  reported as progression); `DEATH` otherwise.
- `CNSDTDSC` is blank for an event; for a censored record it
  is `LAST TUMOUR ASSESSMENT` when censoring falls on the last
  valid tumour assessment date and `START OF NEW ANTI-CANCER
  THERAPY` otherwise.
- `SRCDOM` traces `ADT`: `ADRS` for `DISEASE PROGRESSION` and
  for censoring at `LAST TUMOUR ASSESSMENT`, `DS` for `DEATH`,
  and `ADSL` for censoring at therapy start.
- `SRCVAR` names the source field: `DSSTDTC` when `SRCDOM` is
  `DS`, `ADT` when `SRCDOM` is `ADRS`, and `NACTDT` otherwise.
- `SRCSEQ` holds the source sequence number: the sequence
  number of the record chosen above (lowest on a progression
  tie, highest on a last-assessment tie, earliest death), and
  blank for censoring at therapy start.

**Note:** a new anti-cancer therapy closes observation:
progression or death after the therapy start date never becomes
the event, even when no valid assessment exists, and a record
censored there traces to the subject-level therapy date with a
blank sequence number.

**Standard:** ADaM | **Domain:** ADTTE
