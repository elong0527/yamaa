# Reject Mismatched Lists

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-mismatched.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the lookup pairs two current-row values with three
limit-table columns, so one column would match against nothing.
The two lists pair by position, and a length mismatch is rejected
before any data is read; no artifact is accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Give each current-row value exactly one lookup-table column:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

The two lists pair by position and must have the same length.
