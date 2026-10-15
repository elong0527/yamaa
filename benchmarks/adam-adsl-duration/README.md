# Derive Exposure Duration in Weeks and Months

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-duration.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's exposure start (`STDT`) and end
(`ENDT`) dates, and derive whole-week (`DURW`) and whole-month
(`DURM`) exposure durations.

**Input:** one demographic record per subject carrying the exposure
start and end dates.

**Variables:**

- `DURW` is the count of whole seven-day blocks from `STDT` to
  `ENDT`; a leftover partial week adds nothing.
- `DURM` is the count of monthly anniversaries of `STDT` falling
  on or before `ENDT`; an anniversary keeps the start day, or the
  last day of the month when the month is too short.

**Note:** both durations are missing when either date is missing.
When the end date falls before the start date, each duration is
the negated count computed with the dates exchanged.

**Standard:** ADaM | **Domain:** ADSL
