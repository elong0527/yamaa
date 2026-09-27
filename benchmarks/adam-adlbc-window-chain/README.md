# Carry a Previous Visit's Change

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlbc-window-chain.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the albumin change parameter `_ALB`, carrying each
visit's previous result (`PREV_AVAL`), the change from it (`CHG`),
and the previous visit's change (`PREV2`).

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`). Six
subjects: a four-visit chain that skips a visit number, a subject
whose middle visit has no result, a single-visit subject, a
two-visit subject, a subject with no change between two consecutive
visits, and one whose hemoglobin record produces no row.

**Variables:**

- `AVISITN`: the analysis visit number, taken from the lab visit
  number.
- `AVAL`: the numeric albumin result for the visit.
- `PREV_AVAL`: the `AVAL` of the visit just before in `AVISITN`
  order, for the same subject; blank on the subject's first visit.
- `CHG`: `AVAL - PREV_AVAL`, the change since that visit; blank
  whenever `PREV_AVAL` is.
- `PREV2`: the `CHG` of the visit just before, read after that
  change is complete; blank until two earlier visits carry results.

**Note:** the previous visit's change is carried from its completed
value, so a record with no numeric result shortens the chain: it
gets no row, and the change and its lag compare against the latest
earlier visit with a result. `CHG` here is the change from the
previous visit, not the ADaMIG "change from baseline" definition; a
zero change is a real value (`0`), distinct from blank.

**Standard:** ADaM | **Domain:** ADLBC
