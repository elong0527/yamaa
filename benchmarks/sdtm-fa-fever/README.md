# Fever Occurrence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-fa-fever.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `FASEQ`, `FATESTCD`, `FATEST`, `FACAT`, `FASCAT`,
`FAOBJ`, `FAORRES`, `FASTRESC`, and `VSSTRESN` for each qualifying
temperature record in Findings About (FA).

**Input:** one row per collected Vital Signs (VS) result, carrying
the test code, the category, the numeric result, and the unit.

**Variables:**

- `FASEQ` copies the collected sequence number, so each output row
  keeps the identity of its source record.
- `FATESTCD`, `FATEST`, `FACAT`, `FASCAT`, and `FAOBJ` are always
  `OCCUR`, `Occurrence Indicator`, `REACTOGENICITY`, `SYSTEMIC`, and
  `FEVER`.
- `FAORRES` is `Y` for a temperature of 38 degrees Celsius or higher
  and `N` for a lower one; blank when the result is missing or its
  unit is not Celsius (`C`).
- `FASTRESC` copies `FAORRES`.
- `VSSTRESN` repeats the collected numeric result used for the
  threshold, and is missing when none was collected.

**Note:** only temperature records in the reactogenicity category
qualify; other vital signs records give no row.

**Standard:** SDTM | **Domain:** FA
