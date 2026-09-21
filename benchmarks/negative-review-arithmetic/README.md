# Reject Arithmetic Condition

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-review-arithmetic.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events (AE) for protocol review in
`REVIEWFL`.

**Input:** collected adverse events with onset date (`AESTDTC`).

**Variables:**

- `REVIEWFL`: would contain `Y` when the event sequence number
  plus `1` exceeds `1`, otherwise `N`, but the run is rejected
  before any row is produced because the decision performs
  arithmetic on the sequence number instead of comparing a named
  value.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

State the equivalent comparison directly when no intermediate value is needed:

```yaml
when: "AE.AESEQ > 0"
```
