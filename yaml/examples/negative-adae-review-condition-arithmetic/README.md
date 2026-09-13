# Reject a review flag whose condition performs arithmetic

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adae-review-condition-arithmetic.html)

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
