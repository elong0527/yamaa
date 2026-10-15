# Reject Bad Sex

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-sex-code.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the recorded sex (`SEX`) for each subject from
collected demographics, accepting only `M`, `F`, and `U`.

**Input:** collected demographics with study and subject
identifiers and recorded sex (`SEX`).

**Variables:**

- `SEX` is the subject's recorded sex, copied from collected
  `SEX`. A missing value remains missing and passes the final check;
  any other value must be `M`, `F`, or `U`.

A non-missing code outside the accepted set is rejected after the
dataset is completed, so no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Confirm the code against the study's sex coding convention. Correct `X` at
the governed source when it is a data-entry error. If `X` is an intentional
study code, document that policy and add it to the anchored accepted set
on the `SEX` column:

```yaml
verifications:
  - matches:
      pattern: '^[MFUX]$'
```
