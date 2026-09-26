# Standardize Collected Sex, Race, and Age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-demographics.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** produce standard sex, race code, age, and age group
values from collected demographics, carrying `SEX` and `RACE`
through and producing `SEXN`, `RACEN`, `AGE`, and `AGEGR1`.

**Input:** one record per subject carrying collected `SEX`, `RACE`,
and `AGE` as reported.

**Variables:**

- `SEX` is the collected sex carried through unchanged; a missing
  collected value gives `U`.
- `SEXN` is the numeric sex code: `M` gives `1`, `F` gives `2`, and
  `U` gives `0`.
- `RACEN` is the numeric race code: `WHITE` gives `1`,
  `BLACK OR AFRICAN AMERICAN` gives `2`, `ASIAN` gives `3`, and
  `MULTIPLE` gives `4`; a missing source leaves `RACEN` empty while
  any other reported value gives `99`.
- `AGE` is the collected `AGE` as a whole number; it is missing when
  no age was collected and when the reported value is not a whole
  number.
- `AGEGR1` is the age group: `<18` when age is below 18, `18-64`
  when age is at least 18 and below 65, and `>=65` when age is at
  least 65; it is `UNKNOWN` when age is missing.

**Note:** the numeric mappings match exactly: a race value with
different case or extra wording gives `99`.

**Standard:** ADaM | **Domain:** ADSL
