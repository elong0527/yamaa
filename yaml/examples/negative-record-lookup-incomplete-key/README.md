# Reject an upper-limit lookup with a missing sex

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-record-lookup-incomplete-key.html)

**Goal:** look up the sex-specific reference upper limit
(`LBSTNRHI`) for each collected laboratory result by its test
code and sex.

**Input:** collected results carrying the test code (`LBTESTCD`),
sex (`SEX`), and numeric result (`LBSTRESN`), plus a limits table
carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would be the upper limit from the limits table for
  the matching test code and sex.

One collected result has a blank sex, so its lookup key is
incomplete and the run is rejected with no artifact accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Recover the missing sex when possible. If an incomplete lookup key is
intended to make every value read from the lookup missing, state that policy
on the record lookup:

```yaml
record_lookups:
  - id: REFRANGE
    dataset: LBRANGE
    source: [LBTESTCD, SEX]
    key: [LBTESTCD, SEX]
    incomplete: missing
```

A complete key the table does not contain is a separate condition needing its
own policy.
