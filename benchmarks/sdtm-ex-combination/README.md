# Combination Regimen

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ex-combination.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each administered combination component with its
dose, unit, administration interval, and adjustment reason, adding
`EXSEQ`, `EXTRT`, `EXDOSE`, `EXDOSU`, `EXSTDTC`, `EXENDTC`, and
`EXADJ`.

**Input:** collected exposure records with one row per administered
component, carrying treatment, dose, unit, administration interval,
and adjustment reason.

**Variables:**

- `EXSEQ` is the order of the administration within the subject,
  numbered by start date and then, on the same date, by treatment
  name.
- `EXDOSU` is the dose unit as collected; milligrams (`mg`),
  milligrams per square metre (`mg/m2`), and area under the curve
  (`AUC`) remain distinct.
- `EXADJ` is the adjustment reason as collected (for example
  `DOSE INTERRUPTED`); blank when none was reported.

**Note:** a saline placebo component is administered, so it keeps its
`EXDOSE` of `0`; a zero dose is not an uncollected dose.

**Standard:** SDTM | **Domain:** EX
