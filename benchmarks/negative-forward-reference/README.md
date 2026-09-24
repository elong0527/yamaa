# Reject Forward Reference

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-forward-reference.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the collected analysis value in `AVAL` and add
its doubled value in `AVALDOUBLED`, one record per collected
result.

**Input:** collected laboratory (LB) results carrying the
collected result in `AVAL`, identified by study and subject.

**Variables:**

- `AVAL` would contain the collected result, taken from `AVAL`
  in the laboratory input; missing when the collected result is
  missing.
- `AVALDOUBLED` would contain `AVAL` multiplied by 2; missing
  when the collected result is missing.

The doubled value is listed before the collected value it reads,
and a value may read only values listed before it, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

List `AVAL` before `AVALDOUBLED`, so every value reads only values already
computed.
