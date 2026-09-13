# Represent a combination regimen

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ex-combination-regimen.html)

**Goal:** carry each administered combination component with its
dose, unit, administration interval, and adjustment reason, adding
`EXSEQ`, `EXTRT`, `EXDOSE`, `EXDOSU`, `EXSTDTC`, `EXENDTC`, and
`EXADJ`.

**Input:** collected exposure records with one row per administered
component, carrying treatment, dose, unit, administration interval,
and adjustment reason.

**Variables:**

- `EXSEQ` is the order of the administration within the subject,
  numbered by start date then treatment.
- `EXTRT` is the administered component name as collected.
- `EXDOSE` is the administered dose as collected, including `0`
  for a saline placebo component.
- `EXDOSU` is the dose unit as collected; milligrams (`mg`),
  milligrams per square metre (`mg/m2`), and area under the curve
  (`AUC`) remain distinct.
- `EXSTDTC` is the administration start as collected.
- `EXENDTC` is the administration end as collected.
- `EXADJ` is the adjustment reason as collected (for example
  `DOSE INTERRUPTED`); blank when none was reported.

**Note:** a saline placebo dose of `0` is an administered component
and keeps its zero dose; it is not treated as an uncollected dose.

**Standard:** SDTM | **Domain:** EX
