# Reject a reference limit chosen by an unpaired key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-mapping-from-key-length-mismatch.html)

**Goal:** carry sex and the numeric result into the output and
choose the upper limit of normal (`ANRHI`) from a reference table
by test and sex.

**Input:** collected laboratory results carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal from the reference
  table for the matching test and sex.

**Note:** the lookup lists two current-row values, the test code
and the sex, but pairs them with only one reference-table column,
the test code. Dropping the unpaired value, or pairing it by name,
would each choose a different limit, so the run is rejected before
any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Pair every current-row lookup value with its corresponding lookup-table
column:

```yaml
mapping_from:
  source: [PARAMCD, SEX]
  dataset: LBREF
  key: [LBTESTCD, SEX]
  value: ANRHI
```

The two lists pair by position and must have equal lengths.
