# Ordering

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-ordering.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin the emitted row order: multiple ordering terms, missing
value placement independent of direction, and stable ties.

**Input:** `AE` carries eight shuffled adverse-event records. Two have
missing start days, two share a start day with different sequence
numbers, and two agree on both ordering terms while belonging to
different subjects.

**Order:** the artifact sorts by analysis start day ascending with
missing values first, since missing placement is independent of
direction, then by event sequence number descending. The two records
with missing days swap into sequence-number order; the tied day-five
records follow their sequence numbers; and the two records equal on
both terms keep their construction order, with the rash record before
the hives record as they appear in the input. The decoded term
(`AEDECOD`) rides along untouched, and the nullable analysis start day
(`AESTDY`) drives the sort.

**Standard:** CDISC | **Domain:** ADAE
