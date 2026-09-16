# Keep an implausible age for review

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-age-quality-review.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/yaml/examples/README.md#lifecycle)

**Lifecycle:** draft - first commit, no review yet.

**Goal:** carry each subject's `AGE` into ADSL while retaining an
out-of-range value for later data review.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGE`: the subject's collected age, including a value outside the
  expected range of 18 to 100.

The primary dataset keeps the record. Its review log records
`LOG_VERSION`, `ARTIFACT`, `SEVERITY`, `CONDITION`, `REQUIREMENT`,
`SPEC_PATH`, `VERIFICATION_ID`, `FAILURE_COUNT`, `OFFENDING_KEYS`, and
`DETAILS`, so the source value can be investigated without stopping the run.

**Standard:** ADaM | **Domain:** ADSL
