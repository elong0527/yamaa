# Reject a lookup whose key cannot be inferred

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-record-lookup-no-applicable-keys.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** attach the reference-range upper limit (`LBSTNRHI`) to
each collected result, carrying its test code and sex.

**Input:** collected results carrying test code (`LBTESTCD`), sex
(`SEX`), and numeric result (`LBSTRESN`), plus a reference-limit
table carrying the upper limit (`NRHI`) by test code and sex.

**Variables:**

- `LBSTNRHI` would contain the upper limit value from the
  reference-limit table for the test code and sex of the result.

**Note:** the lookup omits `key`, so the planner tries the output
keys (`STUDYID`, `USUBJID`, `LBSEQ`), but none of them names a
column of the limit table. With no applicable key the match cannot
be inferred, so the run is rejected before any data is read and no
artifact is accepted.

**Standard:** SDTM | **Domain:** LB

## How to fix

Name the lookup-table columns paired with the current-row values:

```yaml
intermediates:
  - id: REFRANGE
    dataset: LBRANGE
    key: [LBTESTCD, SEX]
```

Omit `key_base` when it names the same columns as `key`; omit `key`
only when the output keys name columns of the lookup table.
