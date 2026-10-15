# Calculate each subject's mean result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-mean.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add the subject's mean result (`AVALMEAN`) for each
laboratory parameter to every collected record.

**Input:** collected laboratory (LB) records carrying study,
subject, sequence number, test code, and the collected numeric
result.

**Variables:**

- `AVAL`: the collected numeric result for the record; empty
  when no usable result was collected.
- `AVALMEAN`: the mean of the subject's non-missing `AVAL`
  values for the parameter, repeated on every record for that
  subject and parameter, including a record whose own `AVAL`
  is empty; empty, never zero, when the subject has no
  non-missing `AVAL` for the parameter.

**Note:** the mean is taken separately for each test code, so
each of a subject's records carries the mean for its own test.

**Standard:** ADaM | **Domain:** ADLB
