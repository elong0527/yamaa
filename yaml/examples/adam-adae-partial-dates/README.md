# Impute partial adverse event dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-partial-dates.html)

**Goal:** one Analysis Data Model (ADaM) row per adverse event
(AE) with a completed analysis start date and a treatment-emergence
flag.

**Input:** collected Study Data Tabulation Model (SDTM) event
records carrying the reported term `AETERM` and the collected start
`AESTDTC`, plus the subject-level analysis dataset (ADSL) treatment
start `TRTSDT`.

**Variables:**

- `ASTDT` is the analysis start date. A fully collected date is used
  as it stands; a year and month without a day is completed to the
  15th. A year-only value, a value that is not a date, and a missing
  value give no analysis date.
- `ASTDTC` is the same analysis date written as text, empty when
  there is no analysis date.
- `ASTDTF` is `D` when the day was supplied to complete the analysis
  date. It is empty when the date was collected in full and when no
  analysis date could be formed. The flag therefore always matches
  the date shown.
- `TRTEMFL` is `Y` when the analysis start falls on or after
  `TRTSDT`, and empty when there is no analysis date or it falls
  before `TRTSDT`. A supplied day counts exactly as a collected one
  would, so `ASTDTF` tells a reader which flagged events rested on a
  supplied day.

**Note:** a completed date is never placed before `TRTSDT`. Where the
15th would fall before `TRTSDT`, the event moves forward to the
treatment start, which the collected month still allows. Where the
collected month ends before the treatment start, no day it allows can
satisfy that, so the event is left without an analysis date rather
than moved into a month nobody recorded. A date collected in full is
left exactly as collected even when it falls before `TRTSDT`,
because there is nothing about it to choose.

**Standard:** ADaM | **Domain:** ADAE
