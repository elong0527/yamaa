# Mean Lab Result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-mean.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** derive the subject-level mean result (`AVALMEAN`) for one
laboratory parameter.

**Input:** collected laboratory (LB) records for a single test,
carrying study, subject, sequence number, and test code, with the
collected numeric result.

**Variables:**

- `AVAL`: collected numeric result for the record, missing when no
  usable result was collected.
- `AVALMEAN`: arithmetic mean of the subject's non-missing `AVAL`
  values for the parameter, repeated on every record for that
  subject and parameter, including a record whose own `AVAL` is
  missing; missing when the subject has no non-missing `AVAL`.

**Standard:** ADaM | **Domain:** ADLB
