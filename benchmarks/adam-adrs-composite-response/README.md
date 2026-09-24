# Assign a Composite Responder Status

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adrs-composite-response.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** assign each subject a composite responder status (`AVALC`),
its numeric companion (`AVAL`), and the reason behind the assignment
(`ARSN`), for every analysis visit.

**Input:** percent change from baseline values (`PCHG`) under input
parameter code EASI (Eczema Area and Severity Index), matched to a
subject-level file carrying a serious adverse event flag (`SAEFL`)
and a reason for discontinuation (`DCSREAS`). `PARAMCD` is fixed to
`RESP75` and `PARAM` to `EASI-75 Response`.

**Variables:**

- `AVALC` is the responder status: `RESPONDER`, `NON-RESPONDER`, or
  `NOT EVALUABLE`.
- `ARSN` records which check assigned the status: `SAFETY OR
  DISCONTINUATION RULE`, `COMPONENT MISSING`, `THRESHOLD MET`, or
  `THRESHOLD NOT MET`.
- `AVAL` is 1 for a responder and 0 for a non-responder; it stays
  empty when the subject is not evaluable.

**Note:** the checks apply in a fixed order. A subject with a serious
adverse event or any discontinuation reason is a non-responder
whatever the efficacy value, even a missing one; otherwise a subject
with no efficacy value is not evaluable, one with a percent change of
-75 or less (a reduction of at least 75%) is a responder, and everyone
else is a non-responder.

**Standard:** ADaM | **Domain:** ADRS
