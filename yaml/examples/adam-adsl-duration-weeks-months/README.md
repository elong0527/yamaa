# Exposure duration in weeks and months

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-duration-weeks-months.html)

**Goal:** derive whole-week (`DURW`) and whole-month (`DURM`)
exposure durations, carrying the exposure start (`STDT`) and end
(`ENDT`) dates.

**Input:** one demographic record per subject carrying the exposure
start and end dates.

**Variables:**

- `DURW` is the count of whole seven-day blocks from `STDT` to
  `ENDT`; a leftover partial week adds nothing, and the count is
  missing when either date is absent.
- `DURM` is the count of monthly anniversaries of `STDT` falling
  on or before `ENDT`; an anniversary keeps the start day, or the
  last day of the month where the month is too short, and the
  count is missing when either date is absent.

**Note:** when the end date falls before the start date, each
duration is the negated count computed with the dates exchanged.

**Standard:** ADaM | **Domain:** ADSL
