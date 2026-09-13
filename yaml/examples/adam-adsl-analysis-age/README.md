# ADaM ADSL: analysis age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-analysis-age.html)

This example derives the analysis age from the demographic birth date and the
reference randomization date:

- `BRTHDT` and `RANDDT` are carried through as given: the subject's birth date
  and randomization date;
- `AAGE` is the subject's age in whole years between their birth and
  randomization, or missing when either date is absent.
- `AAGEU` is the unit of the analysis age, fixed to `YEARS`.
