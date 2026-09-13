# Reject a reference limit matched against nothing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-unpaired-key.html)

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the current-row values are paired with the limit table
by test code and sex, but nothing says which columns of the limit
table those values are matched against; guessing by matching names
would make a rule out of a coincidence of naming, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Name the lookup-table columns paired with the current-row values:

```yaml
record_lookups:
  - id: REFRANGE
    dataset: LBRANGE
    source: [LBTESTCD, SEX]
    key: [LBTESTCD, SEX]
```

The two lists pair by position and must always be stated together.
