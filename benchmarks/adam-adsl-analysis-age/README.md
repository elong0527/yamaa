# Analysis Age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-analysis-age.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive each subject's analysis age (`AAGE`) and its unit
(`AAGEU`) at randomization for the subject-level dataset.

**Input:** demographics (DM) records carrying the birth date
(`BRTHDT`) and the randomization date (`RANDDT`); both dates are
carried through unchanged.

**Variables:**

- `AAGE` is the count of yearly anniversaries of `BRTHDT` falling
  on or before `RANDDT`; missing when either date is absent. A
  February 29 birthday falls on February 28 in common years, and a
  randomization date before the birth date gives the negated count
  with the dates exchanged.

**Note:** `AAGEU` stays `YEARS` on every record, even where `AAGE`
is missing for absent dates.

**Standard:** ADaM | **Domain:** ADSL
