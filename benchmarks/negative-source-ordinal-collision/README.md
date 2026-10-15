# Reject a source ordinal name collision

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-source-ordinal-collision.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** preserve collected values when original record positions are
requested for review.

**Input:** a listing with record labels and a collected position field.

**Expected:** the run is rejected when a generated position would replace a
collected field. The stored values are never overwritten or renamed.

## How to fix

Choose an ordinal name that is absent from the stored input fields, such as
`ordinal: OriginalPosition`, and reference `SOURCE.OriginalPosition` in the
consuming expressions. See
[REQ-1294](../../rules/storage/ingestion.md#req-1294).
