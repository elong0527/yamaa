# Reject a result with no reference range

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-mapping-from-unmapped-key.html)

**Goal:** carry sex and the collected result into the output and
attach the upper limit of normal (`ANRHI`) chosen by test and sex.

**Input:** collected laboratory results carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal for that test and
  sex, read from the reference table where the collected test code
  and sex match a table entry. The table has no entry for one
  collected test and sex, and no answer is stated for that case.
  Leaving the limit missing would present an out-of-range result
  as unclassified rather than as unchecked, so the run is rejected
  with no artifact accepted.

**Note:** a test code or sex that was never collected is a
different condition from a complete pair the reference table does
not cover, and each is answered separately.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Add the governed `AST/M` reference range to the reference table when one
exists. If the analysis intentionally leaves the limit missing when a complete
key is not in the table, state that policy explicitly:

```yaml
mapping_from:
  source: [PARAMCD, SEX]
  dataset: LBREF
  key: [LBTESTCD, SEX]
  value: ANRHI
  unmapped: null
```

The missing-value answer does not apply to an incomplete key; an incomplete
key has its own answer.
