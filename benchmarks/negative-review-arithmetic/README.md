# Reject Date Arithmetic in a Condition

**Goal:** flag adverse events (AE) whose onset falls inside a 30-day
post-treatment review window in `REVIEWFL`.

**Input:** collected adverse events with onset date (`AESTDTC`) and the
end of treatment (`TRTEDT`).

**Variables:**

- `REVIEWFL` would contain `Y` when the onset is no more than 30 days
  after treatment ended, otherwise `N`.

**Note:** the condition adds 30 to the treatment end date, but
arithmetic takes numbers and `TRTEDT` is a date, so the run is rejected
before any data is read and no artifact is accepted.

## How to fix

Carry the day count in its own column and compare the named value:

```yaml
- name: WINDOWDY
  type: int
  label: Days from Treatment End to Onset
  derivation:
    diff:
      type: date
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
      missing: "N"
```
