# Derive fever occurrence from temperature results

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-fa-fever-occurrence.html)

**Goal:** derive `FASEQ`, `FATESTCD`, `FATEST`, `FACAT`, `FASCAT`,
`FAOBJ`, `FAORRES`, `FASTRESC`, and `VSSTRESN` for each qualifying
temperature record in Findings About (FA).

**Input:** one row per collected Vital Signs (VS) result, carrying
the test code, the category, the numeric result, and the unit.

**Variables:**

- `FASEQ` copies the collected sequence number, so each output row
  keeps the identity of its source record.
- `FATESTCD` is always `OCCUR`.
- `FATEST` is always `Occurrence Indicator`.
- `FACAT` is always `REACTOGENICITY`.
- `FASCAT` is always `SYSTEMIC`.
- `FAOBJ` is always `FEVER`.
- `FAORRES` is `Y` when the unit is Celsius (`C`) and the numeric
  result is 38 or higher, `N` when the unit is Celsius (`C`) and
  the result is lower, and blank when the numeric result is missing
  or the unit is not Celsius (`C`).
- `FASTRESC` copies `FAORRES`, and is blank when `FAORRES` is
  blank.
- `VSSTRESN` repeats the collected numeric result used for the
  threshold, and is missing when none was collected.

**Note:** only temperature reactogenicity records qualify.

**Standard:** SDTM | **Domain:** FA
