# Completion Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-completion-flag.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag subjects who completed the study, adding `COMPFL`
and carrying `TRTSDT` through.

**Input:** subject-level records carrying first treatment date
(`TRTSDT`) when treated, plus disposition records carrying
standardized outcome (`DSDECOD`) and collection date (`DSDTC`).

**Variables:**

- `COMPFL`: `Y` when the subject has a disposition record with
  standardized outcome `COMPLETED`; `N` otherwise. A subject
  whose records carry only other outcomes, such as
  `ADVERSE EVENT`, and a subject with no disposition record
  at all, are both `N`. A subject with two `COMPLETED` records
  stops the run rather than being flagged.

**Note:** the flag answers whether a completion record exists,
not whether its collection date was filled in or how a later
period ended, so a subject who completed and later discontinued
keeps `Y`.

**Standard:** ADaM | **Domain:** ADSL
