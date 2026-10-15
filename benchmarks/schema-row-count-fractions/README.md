# Row Count Bounds

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-row-count-fractions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** bound the rows a filter matches, by count and by share: upper
and lower count bounds, upper and lower share bounds, per-group share
bounds with an inclusive edge, and a deliberately violated warning.

**Input:** eight subjects in two arms of four. Each has an `ARM`, a
`FLAG` marking it for review, and a collected `VAL`. Three are flagged
(one in arm A, two in arm B); seven carry a value and one does not.

**Checks:**

- Flagged subjects stay at or under one half: 3 of 8 do, so the check holds.
- Subjects with a collected value stay above three quarters: 7 of 8 do,
  so the check holds.
- Per arm, flagged subjects stay at or under one half: arm A has 1 of 4,
  arm B has 2 of 4, and the exact half counts as held.
- Flagged subjects number at most three: exactly 3 do, so the check
  holds and pins the inclusive count boundary.
- Subjects with a collected value number at least seven: exactly 7 do,
  so the check holds and pins the inclusive count boundary.
- Unflagged subjects stay at or under one half: 5 of 8 do not, so the
  check is violated. It is a warning, so the run still succeeds and the
  warning log carries the broken check.

The check log records `REPORT_VERSION`, `ARTIFACT`, `SPEC_PATH`,
`VERIFICATION_ID`, `CHECK`, `TARGET`, `REQUIREMENT`, `SEVERITY`,
`OUTCOME`, `CONDITION`, `EVALUATED_COUNT`, `FAILURE_COUNT`, and
`DETAILS`. The group check evaluates two groups; the others evaluate the
whole dataset as one unit. A held check leaves `CONDITION` empty and
reports zero failures; the violated warning names its condition and
reports how many rows matched out of how many.

The warning log records `LOG_VERSION`, `ARTIFACT`, `SEVERITY`,
`CONDITION`, `REQUIREMENT`, `SPEC_PATH`, `VERIFICATION_ID`,
`FAILURE_COUNT`, `OFFENDING_KEYS`, and `DETAILS`. A dataset-level check
has no offending rows, so it carries an empty key set.

**Standard:** ADaM | **Domain:** ADSL
