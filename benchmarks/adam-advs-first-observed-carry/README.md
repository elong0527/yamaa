# Carry the First Observed Vital-Sign Result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-first-observed-carry.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** retain each subject's first observed value for a vital-sign
parameter and carry it to later visits as `BASEVAL`.

**Input:** vital-sign (VS) records with a parameter, visit number, and
numeric result. A visit without a result still has a record.

**Variables:**

- `AVISITN`: the visit number of the measurement.
- `AVAL`: the observed result, blank when no result was collected.
- `BASEVAL`: the first observed result for the subject and parameter,
  repeated on that and every later visit; blank before the first
  observed result.

**Note:** an uncollected first visit does not prevent a later visit from
supplying the value carried forward.

**Standard:** ADaM | **Domain:** ADVS
