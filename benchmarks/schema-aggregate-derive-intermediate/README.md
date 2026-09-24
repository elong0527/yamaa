# Aggregate Derive Reads an Intermediate

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-aggregate-derive-intermediate.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the aggregate `derive` step for #789: a
binding reads a named intermediate that declares `keep`, so one
row-scoped value is visible to every record the aggregate reduces.

> **Engine coverage:** the Python engine implements this behavior.
> The R engine does not implement it yet.

**Input:** exposure records with sequence numbers (`EXSEQ`) and
doses (`EXDOSE`), plus dose caps (`CAPDOSE`), which a subject may
have more than one of.

**Variables:**

- `CUMDOSE`: the cumulative dose over all of the subject's exposure
  records, after capping each record's dose at the subject's lowest
  cap. A subject with no cap has their doses summed uncapped, and a
  record with a missing dose counts as the full cap.

**Mechanism:** the `DOSECAP` intermediate sorts each subject's cap
records by `CAPDOSE` and keeps the first, the lowest cap, so it
holds one record per subject. The `derive` step binds `CAP` from
`DOSECAP.CAPDOSE` for each exposure record, caps that record's
`DOSE` with `least`, which ignores a missing value, and the reducer
sums the capped values. The `keep` declaration is what makes the
intermediate read valid. Each output row is built from the
subject's first exposure record (`EXSEQ` 1), one row per subject.

**Standard:** ADaM | **Domain:** ADEX
