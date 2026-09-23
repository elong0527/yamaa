# Last Alive Date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-alive-date.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `LSTCNTDT` and `LSTALVDT` for each subject from the
contact, treatment, adverse event, and vital signs dates.

**Input:** subject-level records carrying `TRTEDT` and the collected
contact text `LSTCNTDC`, alongside adverse event end dates (`AENDT` in
ADAE) and vital signs dates (`ADATE` in ADVS).

**Variables:**

- `LSTCNTDT` is the contact text completed to a day: a year and month
  take the last day of that month, a year alone takes the last day of
  December, and missing or unusable text leaves the date missing.
- `LSTALVDT` is the latest of `TRTEDT`, `LSTCNTDT`, the subject's
  latest `AENDT`, and the subject's latest `ADATE`. A completed date
  competes on the day it names. When every source is missing the date
  stays missing; otherwise the latest available date is kept.

**Note:** completing a partial contact to the end of its month or
year can make it the latest date, ahead of dates collected in full
within that period: `2025-02` counts as `2025-02-28` and `2025` as
`2025-12-31`.

**Standard:** ADaM | **Domain:** ADSL
