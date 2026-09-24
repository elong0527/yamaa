# Reject Unmapped Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-mapping-unmapped-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry sex and the collected result into the output and
attach the upper limit of normal (`ANRHI`) chosen by test and sex.

**Input:** collected laboratory results carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal for that test and
  sex, read from the reference table where the collected test code
  and sex match a table entry. A collected test and sex with no
  table entry has no stated answer. Leaving the limit missing would
  hide that the result was never checked against a range, so the
  run is rejected with no artifact accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Add the governed `AST/M` reference range to the reference table when one
exists. If the analysis intentionally leaves the limit missing when a complete
key is not in the table, replace `strict: true` with that missing-value
answer:

```yaml
lookup:
  key_base: [PARAMCD, SEX]
  dataset: LBREF
  key: [LBTESTCD, SEX]
  value: ANRHI
  missing: null
```

The same answer also covers a result whose test code or sex is blank: both
find no entry (REQ-0129, REQ-0131).
