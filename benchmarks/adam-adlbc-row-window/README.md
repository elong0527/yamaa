# Change Parameters Built During Row Construction

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlbc-row-window.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the change parameters `_ALB` and `_BILI` from albumin and
bilirubin results, adding the previous visit's value (`PREV_AVAL`) and the
change from it (`CHG`).

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`).

**Variables:**

- `PREV_AVAL`: the `AVAL` of the visit just before in `AVISITN` order,
  for the same subject and parameter, so an `_ALB` row never reads a
  `_BILI` value; blank on the subject's first visit for the parameter.
- `CHG`: `AVAL - PREV_AVAL`, the change since that visit; blank whenever
  `PREV_AVAL` is.

**Note:** a record with no numeric result gets no row, so the previous
value and the change skip that visit and compare with the latest earlier
visit that has a result.

**Standard:** ADaM | **Domain:** ADLBC
