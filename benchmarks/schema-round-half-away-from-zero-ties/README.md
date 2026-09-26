# Pin ties half away from zero

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-round-half-away-from-zero-ties.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `RND1` and `RND0` by rounding collected values,
with halfway values moving away from zero.

**Input:** collected analysis values placed at, just below, and
clear of halfway marks.

**Variables:**

- `VAL1`: the unrounded value feeding the one-decimal-place
  result; empty when the value was not recorded.
- `VAL0`: the unrounded value feeding the whole-number result;
  empty when the value was not recorded.
- `RND1`: `VAL1` rounded to one decimal place; a value exactly
  halfway between two candidates moves away from zero.
- `RND0`: `VAL0` rounded to a whole number; a value exactly
  halfway between two candidates moves away from zero.

**Note:** a value just short of a halfway mark still counts as a
tie -- and still moves away from zero -- when it falls within the
rule's small binary tolerance below the mark; further below, the
value rounds to the nearer candidate. A result that would print as
negative zero is written as zero, and a missing input stays
missing.

**Standard:** ADaM | **Domain:** ADSL
