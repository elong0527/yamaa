# Assemble adverse event dates at collected precision

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ae-partial-dates.html)

**Goal:** build one Study Data Tabulation Model (SDTM) adverse event (AE)
record per collected event, assembling `AESTDTC` and `AEENDTC` at exactly
the precision collected and attaching study days only when the date is
complete.

**Input:** collected adverse-event rows carrying the reported term
(`AETERM`) and separate year, month, and day fields for start and end,
plus the demographics dataset (`DM`) reference start date (`RFSTDTC`).

**Variables:**

- `AESTDTC` is the collected start date assembled as ISO 8601 text. A
  known day, month, and year becomes `YYYY-MM-DD`; a known month and year
  becomes `YYYY-MM`; a known year alone becomes `YYYY`. Nothing below the
  first unknown component is carried forward, so a year with a day but no
  month is still reported as a year only. When every component is missing,
  `AESTDTC` is empty.
- `AEENDTC` is the same assembly applied to the collected end date
  components.
- `AESTDY` is the study day of the start date, calculated from
  `DM.RFSTDTC`, but only when `AESTDTC` is a complete date. There is no
  day zero: an event on the reference date is study day 1 and an event
  before it is a negative study day. When the start date is partial or
  missing, `AESTDY` is empty.
- `AEENDY` is the study day of the end date under the same rules.

**Note:** this is an SDTM example, not the ADaM `adam-adae-partial-dates`
example. It preserves partial dates as collected and does not impute any
missing day or month. Study-day values are therefore absent whenever the
corresponding date is not fully known.

**Standard:** SDTM | **Domain:** AE
