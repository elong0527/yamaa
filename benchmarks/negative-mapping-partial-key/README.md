# Reject Partial Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-mapping-partial-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry sex and the collected result into the output and
attach the upper limit of normal (`ANRHI`) chosen by test and sex.

**Input:** laboratory records carrying test code (`LBTESTCD`), sex
(`SEX`), and collected result (`LBSTRESN`), plus a limit table
carrying test code, sex, and upper limit (`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal taken from the limit
  table where the test matches the collected test and the sex
  matches the carried sex.

When the carried sex is blank, the pair that chooses a limit is
incomplete and no entry can be looked for, so the run is rejected
with no artifact accepted. Falling back to either sex, or to a
combined limit, would answer with a range the study never stated.

**Note:** a blank lookup value is a different condition from a
complete test and sex the limit table does not cover, and each
needs its own stated answer.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Recover and correct the missing sex when possible. If the intended result is a
missing reference limit whenever any lookup input is missing, state that with
the lookup's missing-value answer:

```yaml
lookup:
  key_base: [PARAMCD, SEX]
  dataset: LBREF
  key: [LBTESTCD, SEX]
  value: ANRHI
  missing: null
```

A complete key that is absent from the reference table is a separate condition
with its own answer.
