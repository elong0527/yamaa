# What Was Checked

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-verification-log.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** record every check a run evaluates, held or violated: one row
per check, in the order the run evaluated it.

**Input:** three subjects with an `AGE` each; one subject's age (214)
falls outside the expected 18-to-100 range.

**Checks:**

- `not_missing` on `AGE`: held, all three ages present.
- `range` on `AGE`: violated, one subject outside 18-to-100; a warning,
  so the run still succeeds and the warning log carries the offending
  subject.
- `unique` on `USUBJID`: held, three distinct subjects.
- `row_count`: held, the run produced rows.

The check log records `REPORT_VERSION`, `ARTIFACT`, `SPEC_PATH`,
`VERIFICATION_ID`, `CHECK`, `TARGET`, `REQUIREMENT`, `SEVERITY`,
`OUTCOME`, `CONDITION`, `EVALUATED_COUNT`, `FAILURE_COUNT`, and
`DETAILS`. A held check leaves `CONDITION` empty, reports zero
failures, and carries no details.

The warning log records `LOG_VERSION`, `ARTIFACT`, `SEVERITY`,
`CONDITION`, `REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`,
`FAILURE_COUNT`, `OFFENDING_KEYS`, and `DETAILS`, so the offending
subject can be investigated without stopping the run.

**Note:** the warning log says what broke; the check log says what was
checked and what happened.

**Standard:** ADaM | **Domain:** ADSL
