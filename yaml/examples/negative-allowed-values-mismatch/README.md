# Reject a recorded sex the study does not recognize

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-allowed-values-mismatch.html)

**Goal:** carry the recorded sex (`SEX`) for each subject from
collected demographics, keeping only the study codes `M` and `F`.

**Input:** collected demographics with study and subject
identifiers and recorded sex (`SEX`).

**Variables:**

- `SEX` is the subject's recorded sex, copied from collected
  `SEX`; only `M` and `F` are recognized, so any other code
  rejects the run at the final check after the values are
  derived, and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Correct the offending record at collection, or widen the accepted
codes when the study genuinely admits a third value:

```yaml
verifications:
  - allowed_values:
      values: [M, F, U]
```
