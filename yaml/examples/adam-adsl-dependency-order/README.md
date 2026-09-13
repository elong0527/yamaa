# Build chained population flags and age groups

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-dependency-order.html)

**Goal:** make the combined flag `POPFL`, the safety and
intent-to-treat (ITT) flags `SAFFL` and `ITTFL`, the first
exposure date `TRTSDT` and randomization date `RANDDT`, the
collected age `AGE`, and the age band `AGEGR1` with age rank
`AGERNK`.

**Input:** demographics records carrying subject age `AGE` and
the randomization date `RANDDT`, plus exposure records carrying
the exposure start date `EXSTDTC`.

**Variables:**

- `POPFL` holds `Y` when both `SAFFL` and `ITTFL` hold `Y`, and
  `N` otherwise, marking subjects in both populations.
- `SAFFL` holds `Y` when `TRTSDT` is present, and `N` otherwise,
  marking subjects who started treatment.
- `ITTFL` holds `Y` when `RANDDT` is present, and `N` otherwise,
  marking subjects who were randomized.
- `TRTSDT` holds the earliest dated `EXSTDTC` for the subject,
  and stays empty when no exposure record carries a date.
- `RANDDT` holds the collected randomization date, and stays
  empty when the subject was not randomized.
- `AGEGR1` holds `<65` when `AGE` is below 65, and `>=65` when
  `AGE` is 65 or above, and stays empty when `AGE` is missing.
- `AGERNK` holds the subject rank by `AGE`, ordered by `AGE`
  then `USUBJID` within each `STUDYID`, starting at 1.
- `AGE` holds the collected subject age.

**Note:** derive the dates and age before the flags that use them,
derive the safety and intent-to-treat flags before the combined
flag, and derive the age band and rank from age.

**Standard:** ADaM | **Domain:** ADSL
