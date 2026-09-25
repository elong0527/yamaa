# Reject Date Arithmetic in a Condition

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-review-arithmetic.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events (AE) whose onset falls inside a 30-day
post-treatment review window in `REVIEWFL`.

**Input:** collected adverse events with onset date (`AESTDTC`) and the
end of treatment (`TRTEDT`).

**Variables:**

- `REVIEWFL` would contain `Y` when the onset is no more than 30 days
  after treatment ended, otherwise `N`.

**Note:** the condition adds 30 days to the treatment end date instead
of comparing named values, so the run is rejected before any data is
read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADAE

## How to fix

Carry the day count in its own column and compare the named value:

```yaml
- name: WINDOWDY
  type: int
  label: Days from Treatment End to Onset
  derivation:
    date_diff:
      start: TRTEDT
      end: ASTDT
      unit: day
- name: REVIEWFL
  type: str
  label: Protocol Review Flag
  derivation:
    flag:
      condition: "WINDOWDY <= 30"
      false_value: "N"
      missing_value: "N"
```
