# Reject Invalid Sex

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-invalid-sex.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the recorded sex (`SEX`) for each subject from
collected demographics, accepting only the study codes `M` and `F`.

**Input:** collected demographics with study and subject
identifiers and recorded sex (`SEX`).

**Variables:**

- `SEX` is the subject's recorded sex, copied from collected
  `SEX`, and blank when none was recorded. Only the exact codes
  `M` and `F` are recognized, so a lowercase `m`, an `X`, or a
  `U` is not accepted.

A code the study does not recognize rejects the run at the final
check, after the values are derived, so no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Correct the offending records at collection: `m`, `X`, and `U` are
not codes the study defines, so query the sites for the subjects'
sex. Widen the accepted codes only when the study genuinely admits
another value, such as `U` for unknown:

```yaml
verifications:
  - allowed_values:
      values: [M, F, U]
```
