# Join a Numeric Sequence to a Padded Supplemental Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-supplb-padded-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the supplemental qualifier value onto each analysis
laboratory row (`QVAL`, `QVAL_INLINE`).

**Input:** laboratory records carry a numeric sequence number; supplemental
records carry the same sequence as eight-character, space-padded text. Two
subjects may share a sequence number.

**Variables:**

- `QVAL` is the qualifier value from the supplemental record whose study,
  subject, and padded sequence all match; empty when nothing matches.
- `QVAL_INLINE` holds the same value from an independent inline match;
  empty when nothing matches.

**Note:** the padding must be exact: a zero-padded key does not match the
space-padded form, and a supplemental record with no laboratory record
changes nothing. Rows in both sequence ranges use the same match.

**Standard:** ADaM | **Domain:** ADLB
