# Reject a review flag that compares a date with text

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adae-review-text-date.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** derive `REVIEWFL` to mark adverse events (AEs)
starting on or after `'2025-01-01'`.

**Input:** collected adverse events carrying `AESTDTC` (event
start date-time).

**Variables:**

- `REVIEWFL` is `Y` when the analysis start date taken from
  `AESTDTC` is on or after `'2025-01-01'`, and `N` otherwise,
  but the run is rejected before any row is produced because the
  comparison tests that calendar date against text.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Declare the constant as a date so both operands carry the same temporal type:

```yaml
when: "ASTDT >= DATE '2025-01-01'"
```
