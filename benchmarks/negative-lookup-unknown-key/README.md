# Reject Uninferable Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-unknown-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the limit-table columns to match on are not stated, so
the match falls back to the output identifiers (`STUDYID`,
`USUBJID`, `LBSEQ`), and the limit table carries none of them. With
nothing to match on, the run is rejected before any data is read and
no artifact is accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Name the lookup-table columns paired with the current-row values:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

This replaces `key_base`: both sides name the same columns, the
current-row values default to the table's names, and writing the same list
twice is rejected. Omit `key` only when the intended match is on the output
keys that the lookup table also carries.
