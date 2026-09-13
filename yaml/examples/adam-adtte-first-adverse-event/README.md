# Time to first adverse event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adtte-first-adverse-event.html)

**Goal:** derive `STARTDT`, `ADT`, `AVAL`, `CNSR`, `EVNTDESC`,
`SRCDOM`, `SRCVAR`, and `SRCSEQ` for the `TTAE` (`Time to First
Adverse Event`) record of each subject, measuring time from
treatment start to the first adverse event (AE), or to end of
study when no event occurred.

**Input:** subject-level treatment start (`TRTSDT`) and
end-of-study (`EOSDT`) dates, with AE records carrying onset
date (`ASTDT`).

**Variables:**

- `STARTDT`: treatment start, copied from `TRTSDT`; missing
  when `TRTSDT` is missing.
- `ADT`: earliest `ASTDT` across the subject's AE records, or
  `EOSDT` when no AE record exists; ties break by lowest
  sequence number. The selected date is moved up to `STARTDT`
  when earlier than treatment start; missing when neither
  source date exists.
- `AVAL`: inclusive day count from `STARTDT` through `ADT`;
  missing when either date is missing.
- `CNSR`: `0` for an AE, `1` when censored at end of study.
- `EVNTDESC`: `AE` for an event, `END OF STUDY` for censoring.
- `SRCDOM`: `ADAE` for an event, `ADSL` for censoring.
- `SRCVAR`: `ASTDT` for an event, `EOSDT` for censoring.
- `SRCSEQ`: sequence number of the selected AE record; blank
  when the record is censored.

**Note:** clamping moves the date only: a selected date earlier
than treatment start is moved up to `STARTDT`, keeping its
censoring, description, and source, with `AVAL` of `1`.

**Standard:** ADaM | **Domain:** ADTTE
