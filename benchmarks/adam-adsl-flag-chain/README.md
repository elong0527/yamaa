# Derive a Chain of Population Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-flag-chain.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the combined population flag `POPFL` from the
safety flag `SAFFL` and the intent-to-treat flag `ITTFL`, plus
the dates and age they read: first exposure date `TRTSDT`,
randomization date `RANDDT`, collected age `AGE`, age band
`AGEGR1`, and age rank `AGERNK`.

**Input:** one demographic record per subject carrying age and
the randomization date, and exposure records carrying the
exposure start date.

**Variables:**

- `POPFL` holds `Y` only when both `SAFFL` and `ITTFL` hold `Y`,
  and `N` otherwise, marking subjects in both populations.
- `SAFFL` holds `Y` when the subject has a first exposure date,
  and `N` otherwise, marking subjects who started treatment.
- `ITTFL` holds `Y` when the subject was randomized, and `N`
  otherwise, marking subjects in the intent-to-treat
  population.
- `TRTSDT` holds the earliest dated exposure start for the
  subject, and stays empty when no exposure record carries a
  date.
- `RANDDT` holds the collected randomization date, and stays
  empty when the subject was not randomized.
- `AGEGR1` holds `<65` when `AGE` is below 65 and `>=65` when it
  is 65 or above; a subject with no age stops the run.
- `AGERNK` holds the subject rank by `AGE` within the study,
  youngest first, with `USUBJID` breaking ties.

**Note:** the dates and age are settled before the flags that
read them: the safety and intent-to-treat flags come from their
dates, and the combined flag comes from both flags.

**Standard:** ADaM | **Domain:** ADSL
