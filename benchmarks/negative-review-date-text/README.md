# Reject Date-Text Comparison

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-review-date-text.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `REVIEWFL` to mark adverse events (AEs)
starting on or after 2025-01-01.

**Input:** collected adverse events carrying `AESTDTC` (event
start date).

**Variables:**

- `REVIEWFL` would be `Y` when the event start date is on or
  after 2025-01-01, and `N` otherwise, including when no start
  date was collected.

**Note:** the cutoff is written as text rather than as a calendar
date, and a date cannot be compared with text, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Declare the constant as a date so both operands carry the same temporal type:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
