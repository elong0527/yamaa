# What Was Checked

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-verification-log.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** record every check a run evaluates, held or violated: one row
per check, in the order the run evaluated it.

**Input:** subject-level records with an `AGE` each; one age (214) falls
outside the expected 18-to-100 range.

**Checks:**

- `not_missing` on `AGE`: held, every age is present.
- `range` on `AGE`: violated by the age outside 18-to-100; it is a
  warning, so the run still succeeds and the warning log carries the
  offending subject.
- `unique` on `USUBJID`: held, no subject repeats.
- `row_count`: held, the run produced at least one row.

The check log records `REPORT_VERSION`, `ARTIFACT`, `SPEC_PATH`,
`VERIFICATION_ID`, `CHECK`, `TARGET`, `REQUIREMENT`, `SEVERITY`,
`OUTCOME`, `CONDITION`, `EVALUATED_COUNT`, `FAILURE_COUNT`, and
`DETAILS`. `EVALUATED_COUNT` counts what the check examines: rows for a
row-by-row check, distinct subjects for `unique`, and the whole dataset
as one unit for `row_count`. A held check leaves `CONDITION` empty,
reports zero failures, and has empty details (`{}`).

The warning log records `LOG_VERSION`, `ARTIFACT`, `SEVERITY`,
`CONDITION`, `REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`,
`FAILURE_COUNT`, `OFFENDING_KEYS`, and `DETAILS`, so the offending
subject can be investigated without stopping the run.

**Note:** the warning log says what broke and for whom; the check log
says what was checked and what happened. The two join on `SPEC_PATH`,
and only the warning log lists the offending subjects.

**Standard:** ADaM | **Domain:** ADSL
