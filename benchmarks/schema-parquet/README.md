# Parquet Everywhere

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-parquet.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry each subject's `AGE` into ADSL as collected, listing a
subject whose age falls outside 18 to 100 in a warning log rather than
stopping the run, with every file in the Parquet format: the input, the
output dataset, and the warning log (`adsl-violations.parquet`).

**Input:** one Parquet file, `input/dm.parquet`, carrying age (`AGE`) per
subject.

**Note:** Parquet carries its own typed fields, so the spec declares no
field types for the input, and a missing value is a null rather than an
empty field. The golden files compare on what the bytes read back as:
field names and order, logical types, row order, nulls, and values,
not on the bytes themselves.

**Standard:** ADaM | **Domain:** ADSL
