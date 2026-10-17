# Reject Below-Limit Value

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

## How to fix

Retain the reported text in a string column and state what the numeric analysis
value should be. If a result such as `<50` is intentionally represented as a
missing numeric value, handle the failed conversion explicitly:

```yaml
derivation:
  value:
    source: LB.LBSTRESC
  unconvertible: {literal: null}
```

If the study uses a numeric substitution for values below the assay limit,
derive that documented value instead and keep the original character result
for traceability.
