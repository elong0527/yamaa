# Normal Limit Check

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-reported-precision.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** report each analysis value (`AVAL`) against the analysis
normal range lower limit (`ANRLO`), adding `R2ANRLO`.

**Input:** laboratory (LB) records carrying the analysis value
(`AVAL`) and the analysis normal range lower limit (`ANRLO`) for
each test.

**Variables:**

- `R2ANRLO`: the result as a multiple of the lower limit of the
  normal range; blank when the lower limit was not collected or
  was recorded as zero.

**Note:** every number is reported to four places, so a result
that needs fewer still shows them and a ratio that needs more is
rounded exactly once, as it is written, with an exact half going
away from zero (one thirty-second is reported as `0.0313`).
Nothing is rounded before the report: the ratio keeps every digit
it was calculated with, and a number that was never collected is
reported as absent rather than as four zeroes.

**Standard:** ADaM | **Domain:** ADLB
