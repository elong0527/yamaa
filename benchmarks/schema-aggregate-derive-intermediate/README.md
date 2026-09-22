# Aggregate Derive Reads an Intermediate

[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)
[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-aggregate-derive-intermediate.html)

**Goal:** demonstrate the aggregate `derive` step for #789: a
binding reads a named intermediate that declares `keep`, so one
row-scoped value is visible to every record the aggregate reduces.

> **Engine coverage:** the Python engine implements this behavior
> (REQ-1240). The R engine does not implement it yet.

**Input:** exposure records with sequence numbers (`EXSEQ`) and
doses (`EXDOSE`), plus per-subject dose caps (`CAPDOSE`).

**Variables:**

- `STUDYID`: the study identifier, carried through.
- `USUBJID`: the unique subject identifier, carried through.
- `CUMDOSE`: the cumulative dose after capping each record's dose
  at the subject's cap.

**Mechanism:** the `DOSECAP` intermediate keeps the first record
per subject ordered by `CAPDOSE`, so it holds one row per subject.
The `derive` step binds `CAP` from `DOSECAP.CAPDOSE` per record,
caps each record's `DOSE` with `least`, and the reducer sums the
capped values. The `keep` declaration is what makes the
intermediate read valid.

**Standard:** ADaM | **Domain:** ADEX
