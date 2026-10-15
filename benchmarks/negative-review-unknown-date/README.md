# Reject Unknown Date

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

## How to fix

Use the declared analysis start date in the condition:

```yaml
derivation:
  flag:
    condition: "ASTDT >= DATE '2025-01-01'"
    false_value: "N"
    missing: "N"
```
