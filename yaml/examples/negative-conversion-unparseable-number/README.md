# Reject a viral load reported below the assay limit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-conversion-unparseable-number.html)

**Goal:** one record for each subject and collected parameter,
carrying the numeric analysis result (`AVAL`).

**Input:** collected laboratory results carrying the test code
(`LBTESTCD`) and the reported character result (`LBSTRESC`).

**Variables:**

- `AVAL` would hold the reported number read from `LBSTRESC`
  for the collected parameter. A result reported as a limit
  instead of a number cannot be read as that number, as zero, or
  as missing without a stated rule, so the run fails and no
  artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Retain the reported text in a string column and state what the numeric analysis
value should be. If a result such as `<50` is intentionally represented as a
missing numeric value, handle the failed conversion explicitly:

```yaml
derivation:
  value:
    source: LB.LBSTRESC
  conversion_failure: null
```

If the study uses a numeric substitution for values below the assay limit,
derive that documented value instead and keep the original character result
for traceability.
