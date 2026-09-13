# Reject age bands built from a coded value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-cut-non-numeric-source.html)

**Goal:** build `AGEGRP` to place each subject into an age band,
with `younger` below 65 and `elderly` at or above 65.

**Input:** collected demographics carrying numeric age (`AGE`)
and coded sex (`SEX`).

**Variables:**

- `AGEGRP` would be `younger` below 65 and `elderly` at or above
  65 from the coded sex, which comes from `SEX` in the collected
  demographics, but no row is produced.

Placing a coded value into numeric bands has no meaning, so the
run is rejected before any data is read and no artifact is
accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Group by the numeric age rather than the coded sex:

```yaml
cut:
  source: DM.AGE
  breaks: [65]
  labels: [younger, elderly]
```
