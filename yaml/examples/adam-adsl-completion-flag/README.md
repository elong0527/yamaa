# Flag subjects who completed the study

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-completion-flag.html)

**Goal:** flag subjects who completed the study, adding `COMPFL`
and carrying `TRTSDT` through.

**Input:** subject-level records carrying first treatment date
(`TRTSDT`) when treated, plus disposition records carrying
standardized outcome (`DSDECOD`) and collection date (`DSDTC`).

**Variables:**

- `COMPFL`: `Y` when at least one disposition record carries
  standardized outcome `COMPLETED`; `N` otherwise. A subject
  whose records carry only other outcomes, such as
  `ADVERSE EVENT`, and a subject with no disposition record
  at all, are both `N`.

**Note:** the flag answers whether a completion record exists,
not whether its collection date was filled in or how a later
period ended, so a subject who completed and later discontinued
keeps `Y`.

**Standard:** ADaM | **Domain:** ADSL
