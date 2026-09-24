# Reject Numeric Lowercase

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-lowercase-number.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** fold `SITE` to lower case for analysis use.

**Input:** collected demographics carrying the numeric site number
(`SITENUM`).

**Variables:**

- `SITE` would be the site name in lower case, but it is taken from the
  numeric site number (`SITENUM`), which has no lower-case form.

The request is rejected before any data is read, and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Fold the collected site name rather than its number, once `SITENM` is
present in the demographics extract:

```yaml
str_lower:
  source: DM.SITENM
```
