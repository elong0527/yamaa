# Missing Term Yields Unknown Skin Flag

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-str-contains-missing.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose term mentions dermatitis or
erythema, writing the structured `str_contains` result straight into
`SKIN_FLAG`. Here every collected term is missing, so the search has
nothing to test.

**Input:** one record per adverse event carrying `AESEQ` (sequence
number) and `AEDECOD` (dictionary-derived term); both records have no
term.

**Variables:**

- `SKIN_FLAG`: `UNKNOWN` on every row. With no term present, the
  `missing` value is used instead of running the search, so the
  boolean result never appears and the run succeeds.

**Standard:** ADaM | **Domain:** ADAE
