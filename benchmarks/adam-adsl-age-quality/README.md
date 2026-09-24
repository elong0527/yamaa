# Keep Out-of-Range Ages for Review

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-age-quality.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's collected age (`AGE`) into the ADSL analysis
dataset, keeping values outside the expected range of 18 to 100 for later
data review.

**Input:** collected demographics, one record per subject, carrying age.

**Variables:**

- `AGE`: the subject's age in years as collected. A value below 18 or above
  100 stays in the dataset as collected, and a missing age stays missing;
  neither case stops the run.

**Note:** an age outside 18 to 100 does not stop the run: every subject keeps
its record, and the review log records one row for the failed check, naming
the dataset (`ARTIFACT`, `LOG_VERSION`), what was checked (`CONDITION`,
`REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`), how serious the failure is
(`SEVERITY`), and which subjects failed (`FAILURE_COUNT`, `OFFENDING_KEYS`,
`DETAILS`).

**Standard:** ADaM | **Domain:** ADSL
