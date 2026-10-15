# Reject Age Bands for a Missing Age

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-cut-missing-input.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `AGEGRP` to place each subject into an age band,
with `younger` below 65 and `elderly` at or above 65.

**Input:** collected demographics carrying numeric age (`AGE`).

**Variables:**

- `AGEGRP` would place each subject into `younger` (below 65) or
  `elderly` (65 and over). A subject with no recorded age cannot be
  placed into a band and no fallback is stated, so the run is rejected
  with no artifact accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide what the band means for a subject with no recorded age,
then say so. To record the band as unknown for that subject:

```yaml
cut:
  source: AGE
  breaks: [65]
  labels: [younger, elderly]
  missing: unknown
```
