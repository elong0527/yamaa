# Subject Dose Aggregation

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-aggregation.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** summarize each subject's dosing records three ways: a qualified
reduction over the dosing table, a filtered and a per-record-derived
variant of it, and an unqualified reduction over the built rows that is
broadcast back to every row in its treatment group.

**Input:** `DM` carries one row per subject with a treatment group
(`TRTGRP`). `EX` carries the dosing records: subject, sequence, dose,
and treatment. Subject `04` has no dosing records; subject `03` has one
record whose dose is missing; subject `01` has three records, one with a
missing dose.

**Aggregations:**

- `NDOSE` counts dose records (`COUNT(EX.*)`); `NDOSEVAL` counts only the
  records with a dose present (`COUNT(EX.EXDOSE)`). `TOTDOSE`, `MEANDOSE`,
  and `MAXDOSE` reduce the present doses.
- `DRUGDOSE` sums only the `DRUG` records; `PLACDOSE` sums only the
  `PLACEBO` records, so subjects with no placebo record read as missing.
- `TOTDOSE_MG` binds each record's dose in milligrams first (a `derive`
  step whose second binding reads the first) and then sums the bound values.
- `AVGNDOSE` is unqualified: it averages `NDOSE` over the built rows
  within each treatment group and broadcasts the result to every row of
  the group. Subject `04`'s missing `NDOSE` is excluded from the average.

**Note:** a missing dose contributes nothing and never a zero: `TOTDOSE`
for subject `03` is missing while `NDOSEVAL` is `0`. A subject with no
records at all (subject `04`) reads every qualified reduction as missing,
including both counts. An empty group after filtering (`PLACDOSE` for
subject `01`) also reads as missing.

**Standard:** ADaM | **Domain:** ADSL
