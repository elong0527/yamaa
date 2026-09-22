# Derived Intermediate Rank

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-intermediate-derived-filter.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** select each subject's disposition record by ordering on a rank
derived per donor record.

**Input:** one `spec.yaml` declaring two datasets. `DM` carries one row
per subject. `DS` carries disposition records with sequence, study day, and
a representative numeric value.

**Lookups:**

- `EOT` derives `EOT_FALLBACK` from study day and sequence, then filters
  and orders on the derived rank. Records whose derived rank is missing
  are ineligible; among the remaining records, `keep: first` selects
  the smallest derived rank. The selected float demonstrates shortest
  round-trip rendering.

**Columns:** the chosen donor's decoded term lands in `DSDECOD`, its
representative numeric value lands in `DSVALUE`, and the computed rank is
read through `EOT.EOT_FALLBACK` into `EOT_FALLBACK`.

**Standard:** ADaM | **Domain:** ADSL
