# Reject a site name folded from a number

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-str-lower-non-string-source.html)

**Goal:** fold `SITE` to lower case for analysis use.

**Input:** collected demographics carrying the numeric site number
(`SITENUM`).

**Variables:**

- `SITE` (site name) was specified from the numeric site number
  (`SITENUM`), which has no lower-case form, so the run is rejected
  before any data is read and no row is produced.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Fold the collected site name rather than its number, once `SITENM` is
present in the demographics extract:

```yaml
str_lower:
  source: DM.SITENM
```
