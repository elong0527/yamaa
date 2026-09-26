# Carry a Previous Visit's Change

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlbc-window-chain.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the previous albumin result (`PREV_AVAL`), the change
from it (`CHG`), and the previous visit's change (`PREV2`).

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`),
a numeric result (`LBSTRESN`), and a visit number (`VISITNUM`).

**Variables:**

- `AVISITN`: the analysis visit number from the laboratory record.
- `AVAL`: the numeric albumin result at that visit.
- `PREV_AVAL`: the subject's albumin result at the previous visit;
  blank on the first visit.
- `CHG`: the current result minus the previous result; blank on the
  first visit.
- `PREV2`: the previous visit's change; blank until two earlier visits
  provide results.

**Note:** the previous visit's change is carried from its completed
value, so the first nonblank `PREV2` appears on the third visit.

**Standard:** ADaM | **Domain:** ADLBC
