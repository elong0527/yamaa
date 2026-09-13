# Reject a collected result with no reference entry

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-unmatched-key.html)

**Goal:** attach the reference upper limit (`LBSTNRHI`) to each
collected result by test and sex.

**Input:** collected results carrying test code (`LBTESTCD`) and
sex (`SEX`), plus a reference table carrying an upper limit
(`NRHI`) per test code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit read from the reference
  table matched on test code and sex. One collected combination
  has no reference entry, and no answer is stated for that case,
  so the run is rejected with no artifact accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Add the governed `AST/M` range to the reference table when one exists. If an
absent range is intentionally represented by a missing value, state that policy
explicitly:

```yaml
record_lookups:
  - id: REFRANGE
    dataset: LBRANGE
    source: [LBTESTCD, SEX]
    key: [LBTESTCD, SEX]
    unmatched: missing
```

The lookup then returns a missing value for every column read through it when
a complete key has no match.
