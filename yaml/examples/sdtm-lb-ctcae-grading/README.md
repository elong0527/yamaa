# Assign laboratory toxicity grades

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-lb-ctcae-grading.html)

**Goal:** build one laboratory record per collected result,
carrying `LBTESTCD`, `SEX`, `LBSTRESN`, and `ATOXGR` from the
Common Terminology Criteria for Adverse Events (CTCAE) bands for
that test.

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
  `2`, `1`, or `0`, from the band set for that test and sex.

**Note:** the assigned grade equals the independently recorded
grade on every record, and each test and sex combination carries
its own band set, so a new combination needs its bands stated
explicitly.

**Standard:** SDTM | **Domain:** LB
