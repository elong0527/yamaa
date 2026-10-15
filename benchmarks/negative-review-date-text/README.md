# Reject Date-Text Comparison

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

## How to fix

Declare the constant as a date so both operands carry the same temporal type:

```yaml
derivation:
  flag:
    condition: "ASTDT >= DATE '2025-01-01'"
    false_value: "N"
    missing: "N"
```
