# Boolean Substring Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-str-contains.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag adverse events whose dictionary-derived term contains
one of the dermatologic criterion terms (`APPLICATION`,
`DERMATITIS`, `ERYTHEMA`, or `BLISTER`) -- the CQ01NAM-style test
from #778.

**Input:** one record per adverse event carrying `AESEQ` (sequence
number) and `AEDECOD` (dictionary-derived term).

**Variables:**

- `CRIT1FL`: `Y` when the term matches any criterion word, and
  missing otherwise.
- `DERMFL`: `Y` when the term mentions a dermatitis-like or
  erythema-like word, and missing otherwise.

**Note:** an event with no recorded term stays unflagged rather than
counting as a non-match, and the criterion words are matched as a
single pattern with alternatives.

**Standard:** ADaM | **Domain:** ADAE
