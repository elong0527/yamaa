# Reject Numeric Sentence Case

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-sentence-number.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive a sentence-case `SITE` for analysis use.

**Input:** collected demographics carrying the numeric site number
(`SITENUM`).

**Variables:**

- `SITE` would be the site name in sentence case, but it is taken from
  the numeric site number (`SITENUM`), which has no sentence-case form.

The request is rejected before any data is read, and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Apply sentence case to the collected site name rather than its number,
once `SITENM` is present in the demographics extract:

```yaml
str_sentence:
  source: DM.SITENM
```
