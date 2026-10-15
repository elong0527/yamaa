# Carry the First Observed Vital-Sign Result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-advs-first-observed-carry.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `BASEVAL`, each subject's first observed result
for a vital-sign parameter, carried forward to every later visit.

**Input:** vital-sign records keyed by subject, parameter code, and
visit number, each carrying the collected numeric result. A visit
with no collected result still has a record.

**Variables:**

- `AVISITN`: the visit number of the measurement.
- `AVAL`: the collected numeric result, blank when the visit has
  no collected result.
- `BASEVAL`: the result from the subject's first visit with a
  collected result for the parameter, repeated on that and every
  later visit; blank on visits before any result was collected.

**Note:** the carried value never crosses subjects or parameters:
each subject-parameter pair carries only its own first observed
result.

**Standard:** ADaM | **Domain:** ADVS
