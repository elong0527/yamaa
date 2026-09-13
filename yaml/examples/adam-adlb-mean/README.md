# Mean laboratory result per subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-mean.html)

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
