# Change Parameters Built During Row Construction

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlbc-row-window.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `_`-prefixed change parameters (`_ALB`, `_BILI`) as
constructed rows, where each row's `PREV_AVAL` is the previous visit's
`AVAL` for the same subject and parameter, computed by a window
expression inside the row template.

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`).

**Variables:**

- `PREV_AVAL`: the `AVAL` of the previous visit in `AVISITN` order
  within the same subject and parameter; blank for the first visit of
  each subject-parameter partition.
- `CHG`: `AVAL - PREV_AVAL`, derived after the window pass from the
  window's result.

**Note:** each row template's window partitions only the rows that
template constructs: the `_ALB` template never sees `_BILI` rows, and
subject B's shorter visit series still lags correctly within its own
partition.

**Standard:** ADaM | **Domain:** ADLBC
