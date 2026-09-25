# Reject a Nonnumeric Padding Width

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-str-pad-width.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** format a numeric laboratory sequence as an eight-character key.

**Input:** one laboratory record with a numeric sequence.

**Variables:**

- `IDVARVAL` would be the sequence number padded to eight characters. The
  written width is text, so this output is rejected before data ingestion.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Set the padding width to the number of characters required:

```yaml
str_pad: {source: LB.LBSEQ, width: 8}
```
