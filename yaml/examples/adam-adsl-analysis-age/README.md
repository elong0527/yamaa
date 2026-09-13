# Analysis age at randomization

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-analysis-age.html)

**Goal:** derive each subject's analysis age (`AAGE`) and its unit
(`AAGEU`) at randomization for the subject-level dataset.

**Input:** demographics (DM) records carrying the birth date
(`BRTHDT`) and the randomization date (`RANDDT`); both dates are
carried through unchanged, and the unit is fixed to `YEARS`.

**Variables:**

- `AAGE` is the count of yearly anniversaries of `BRTHDT` falling
  on or before `RANDDT`; missing when either date is absent. A
  February 29 birthday falls on February 28 in common years, and a
  randomization date before the birth date gives the negated count
  with the dates exchanged.

**Note:** `AAGEU` stays `YEARS` on every record, even where `AAGE`
is missing for absent dates.

**Standard:** ADaM | **Domain:** ADSL
