# Reject Unknown Date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-review-unknown-date.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `REVIEWFL` to flag adverse events (AEs) for
protocol review.

**Input:** collected adverse event records carrying the collected
onset date (`AESTDTC`) alongside the study, subject, and event
sequence identifiers.

**Variables:**

- `REVIEWFL` would contain `Y` when the event onset date is on or
  after 2025-01-01 and `N` otherwise, including when no onset date
  was collected.

**Note:** the decision names `UNKNOWNDT` instead of the onset date,
and no declared column provides it, so the run is rejected before
any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Use the declared analysis start date in the condition:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
