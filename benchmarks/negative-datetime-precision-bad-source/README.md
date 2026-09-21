# Reject Datetime Precision from a Partial Date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-datetime-precision-bad-source.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** report whether the first treatment start (`TRTSTPR`) included a
collected time.

**Input:** exposure records carrying a collected start date or date/time.

**Variables:**

- `TRTSTPR`: `S` when the exposure start included a time and `D` when it was a
  complete date with no time.

A year and month identify neither a complete date nor a moment, so they cannot
answer whether only the time is absent. The run stops rather than reporting a
misleading precision.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Recover the exposure day upstream when it was collected. If a date-imputation
policy applies, complete the date separately before asking
`datetime_precision` whether the resulting source carried a time.
