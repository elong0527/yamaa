# Lag a Result Across Visits While Constructing Change Parameters

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlbc-row-window.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the change parameters `_ALB` and `_BILI` from albumin
and bilirubin lab results, carrying each visit's previous result
(`PREV_AVAL`) and the change from it (`CHG`).

**Input:** laboratory (LB) records carrying a test code (`LBTESTCD`), a
numeric result (`LBSTRESN`), and a visit number (`VISITNUM`). Five
subjects: one with both tests across several visits, one with both
tests but a single albumin visit, one whose first albumin visit has
no result plus a single bilirubin visit, one albumin-only, and one
with hemoglobin only, which produces no rows.

**Variables:**

- `AVISITN`: the analysis visit number, taken from the lab visit
  number.
- `AVAL`: the numeric lab result for the visit.
- `PREV_AVAL`: the `AVAL` of the visit just before in `AVISITN`
  order, for the same subject and parameter, so an `_ALB` row never
  reads a `_BILI` value; blank on the subject's first visit for the
  parameter.
- `CHG`: `AVAL - PREV_AVAL`, the change since that visit; blank
  whenever `PREV_AVAL` is.

**Note:** a record with no numeric result gets no row, so the previous
value and the change skip that visit and compare with the latest
earlier visit that has a result. `CHG` here is the change from the
previous visit, not the ADaMIG "change from baseline" definition.
Single-visit subjects have blank `PREV_AVAL` and `CHG`; equal
consecutive values give `CHG` 0.

**Standard:** ADaM | **Domain:** ADLBC
