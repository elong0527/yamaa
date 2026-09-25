# CTCAE v5.0 Grading

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-grading.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one laboratory record per collected neutrophil or
hemoglobin result, carrying `LBTESTCD`, `SEX`, `LBSTRESN`, and
`ATOXGR` from the Common Terminology Criteria for Adverse Events
(CTCAE) v5.0 bands for that test.

**Input:** collected laboratory results with test, sex,
standardized numeric result, and an independently recorded grade
confirming the assigned grade.

**Variables:**

- `LBTESTCD` is the test short name as collected: `ANC` (absolute
  neutrophil count) or `HGB` (hemoglobin).
- `SEX` is the subject's sex as collected, `M` or `F`; it selects
  which hemoglobin band set applies to the record.
- `LBSTRESN` is the standardized numeric result as collected.
- `ATOXGR` is the toxicity grade for the result, one of `4`, `3`,
  `2`, `1`, or `0`, from the band set for that test and sex; each
  band includes its lower limit and excludes its upper one.

**Note:** neutrophil counts use one band set whatever the sex, and
the example lab's lower limit of normal is 1.8 x 10^9/L. The ANC
bands are in 10^9/L: below 0.5 is grade 4, 0.5 to below 1.0 is
grade 3, 1.0 to below 1.5 is grade 2, and 1.5 to below 1.8 is
grade 1. Hemoglobin uses one band set per sex. A result for any other
test, or a hemoglobin result with a sex other than `M` or `F`, gets no record.
A missing result, or an assigned grade that differs from the
independently recorded one, stops the run.

**Standard:** SDTM | **Domain:** LB
