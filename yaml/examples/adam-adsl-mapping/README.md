# Standardize sex, race, and age for analysis

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-mapping.html)

**Goal:** produce standard sex, race code, age, and age group
values from collected demographics, carrying `RACE` through
unchanged and producing `SEX`, `SEXN`, `SEXDECOD`, `RACEN`, `AGE`,
and `AGEGR1`.

**Input:** one record per subject carrying collected `SEX`, `RACE`,
and `AGE` as reported.

**Variables:**

- `SEX` is the standard sex code, matched without regard to case:
  `M` gives `M`, `F` gives `F`, and `U` gives `U`; a missing source
  and any other reported value both give `U`.
- `SEXN` is the numeric sex code, matched without regard to case:
  `M` gives `1`, `F` gives `2`, and `U` gives `0`; a missing source
  and any other reported value both give `0`.
- `SEXDECOD` is the display form of sex, matched without regard to
  case: `M` gives `Male`, `F` gives `Female`, and `U` gives
  `Unknown`; a missing source and any other reported value both
  give `Unknown`.
- `RACEN` is the numeric race code, matched exactly as reported:
  `WHITE` gives `1`, `BLACK OR AFRICAN AMERICAN` gives `2`, and
  `ASIAN` gives `3`; a missing source and any other reported value
  both give `99`.
- `AGE` is the collected `AGE` as a whole number; it is missing when
  the source is absent and when the reported value is not a number.
- `AGEGR1` is the age group: `<18` when age is below 18, `18-64`
  when age is at least 18 and below 65, and `>=65` when age is at
  least 65; it is `UNKNOWN` when age is missing.

**Note:** sex matching ignores case while race matching is exact, so
a lowercase sex code still maps to its standard form but a race
value with different case or extra wording does not.

**Standard:** ADaM | **Domain:** ADSL
