# Join a Numeric Sequence to a Padded Supplemental Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-supplb-padded-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry a supplemental laboratory value onto the matching analysis row.

**Input:** laboratory records carry numeric `LBSEQ`; SUPPLB carries its
eight-character, space-padded text form in `IDVARVAL`. Subjects may share a
sequence number, so all three key parts must match.

**Variables:**

- `QVAL` is the supplemental value for the matching laboratory sequence;
  empty when no supplemental record matches.
- `QVAL_INLINE` holds the same supplemental value from a direct lookup;
  empty when no supplemental record matches.

**Note:** laboratory rows in different sequence ranges use the same
supplemental match. The numeric sequence is padded to eight characters for
comparison without a separate output column.

**Standard:** ADaM | **Domain:** ADLB
