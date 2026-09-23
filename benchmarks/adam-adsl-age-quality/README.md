# Age Quality Review

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-age-quality.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's `AGE` into ADSL while retaining an
out-of-range value for later data review.

**Input:** collected demographics carrying age (`AGE`).

**Variables:**

- `AGE`: the subject's collected age, including a value outside the
  expected range of 18 to 100.

**Note:** an age outside that range does not stop the run: every
subject keeps its record, and one review log row lists the subjects
outside the range in `OFFENDING_KEYS`, with `FAILURE_COUNT` saying how
many, beside `LOG_VERSION`, `ARTIFACT`, `SEVERITY`, `CONDITION`,
`REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`, and `DETAILS`.

**Standard:** ADaM | **Domain:** ADSL
