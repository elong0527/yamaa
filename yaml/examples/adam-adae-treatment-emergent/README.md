# Flag treatment-emergent events

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-treatment-emergent.html)

**Goal:** flag each adverse event (AE) as treatment-emergent in
`TRTEMFL`.

**Input:** AE records with analysis start date `ASTDT`, plus the
subject-level analysis dataset (ADSL) treatment dates and actual
treatment.

**Variables:**

- `TRTSDT`, `TRTEDT`, `TRTA`: carried through from ADSL; stay empty
  when there is no ADSL record.
- `TRTEMFL`: `Y` when `ASTDT` falls on or between `TRTSDT` and
  `TRTEDT`; otherwise empty, including events before treatment
  started, after it ended, or with missing treatment dates.

**Note:** first and last treatment days count as inside, per this
study's rule.

**Standard:** ADaM | **Domain:** ADAE
