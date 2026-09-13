# Duration of response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtte-duration-of-response.html)

**Goal:** build duration of response (`DOR`) records labeled
`Duration of Response`, one for each responder, carrying
`STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`, `CNSDTDSC`,
`SRCDOM`, `SRCVAR` and `SRCSEQ`.

**Input:** combine a subject-level table holding `RESPDT` and
`NACTDT` with response assessments holding `ADT` and `AVALC`
plus record sequence number `ASEQ`, and a disposition table
holding `DSDECOD`, `DSSTDTC` and `DSSEQ`. Only subjects whose
`RESPDT` is not blank contribute a record.

**Variables:**

- `STARTDT` is the response start date, copied from input
  `RESPDT`.
- `ADT` is the analysis date: the event date when a counted
  event exists, else the earlier of the last valid tumour
  assessment date and input `NACTDT`. The progression date is
  the earliest input `ADT` with `AVALC` equal to progressive
  disease (`PD`); the death date is the earliest input
  `DSSTDTC` with `DSDECOD` equal to `DEATH`. The earlier of
  the two is the event date, unless it falls after input
  `NACTDT`; an event on the `NACTDT` day still counts. The
  last valid tumour assessment date is the latest input `ADT`
  carrying an `AVALC` value other than not evaluable (`NE`).
- `AVAL` is the inclusive day count from `STARTDT` through
  `ADT`, counting both endpoints; it is at least `1`.
- `CNSR` is `0` when a counted event exists and `1` when the
  record is censored.
- `EVNTDESC` names the outcome: `CENSORED` when no event
  counts, `DISEASE PROGRESSION` when the event date equals the
  progression date, else `DEATH`.
- `CNSDTDSC` is blank for an event; for a censored record it
  is `LAST TUMOUR ASSESSMENT` when censoring falls on the last
  valid tumour assessment date and `START OF NEW ANTI-CANCER
  THERAPY` otherwise.
- `SRCDOM` traces `ADT`: `ADRS` for `DISEASE PROGRESSION` and
  for censoring at `LAST TUMOUR ASSESSMENT`, `DS` for `DEATH`,
  and `ADSL` for censoring at therapy start.
- `SRCVAR` names the source field: `DSSTDTC` when `SRCDOM` is
  `DS`, `ADT` when `SRCDOM` is `ADRS`, and `NACTDT` otherwise.
- `SRCSEQ` holds the source sequence number: the `ASEQ` of the
  earliest progression record for `DISEASE PROGRESSION`, the
  `DSSEQ` of the earliest death record for `DEATH`, the `ASEQ`
  of the latest valid assessment record for `LAST TUMOUR
  ASSESSMENT`, and blank for censoring at therapy start.

**Note:** a new anti-cancer therapy closes observation:
progression or death after input `NACTDT` never becomes the
event, and a record censored there traces to the subject-level
therapy date with a blank sequence number.

**Standard:** ADaM | **Domain:** ADTTE
