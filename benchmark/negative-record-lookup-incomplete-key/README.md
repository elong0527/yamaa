# Reject an upper-limit lookup with a missing sex

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-record-lookup-incomplete-key.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

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
intended to make every value read from the lookup missing, omit `strict:`
so the lookup answers with its declared absence:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

REQ-0124 gives an incomplete key and a complete key the table does not
contain the one absence policy: both yield nothing, and `strict:` decides
whether that fails or answers `missing:`.
