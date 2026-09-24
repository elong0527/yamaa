# Reject Below-Limit Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-number-below-limit.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one record for each subject and collected parameter,
carrying the numeric analysis result (`AVAL`).

**Input:** collected laboratory results carrying the test code
(`LBTESTCD`) and the reported character result (`LBSTRESC`).

**Variables:**

- `AVAL` would hold the reported number read from `LBSTRESC`
  for the collected parameter, and is missing when no result is
  reported. A result reported as a limit (such as `<50`) instead
  of a number cannot be read as the limit, as zero, or as missing
  without a stated rule, so the run fails and no artifact is
  accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Retain the reported text in a string column and state what the numeric analysis
value should be. If a result such as `<50` is intentionally represented as a
missing numeric value, handle the failed conversion explicitly in place of
`strict: true`:

```yaml
derivation:
  value:
    source: LB.LBSTRESC
  missing: null
```

If the study uses a numeric substitution for values below the assay limit,
derive that documented value instead and keep the original character result
for traceability.
