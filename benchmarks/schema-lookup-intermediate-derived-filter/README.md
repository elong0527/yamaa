# Derived Intermediate Rank

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-intermediate-derived-filter.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** select each subject's disposition record by ordering on a rank
derived per donor record.

**Input:** one `spec.yaml` declaring two datasets. `DM` carries one row
per subject. `DS` carries disposition records with sequence, study day, and
a representative numeric value.

**Lookups:**

- `EOT` derives `EOT_FALLBACK` for each disposition record: the study
  day plus the sequence number divided by 1000, so the earlier study day
  ranks first and the sequence breaks a tie within a day. It then
  filters and orders on that derived rank. A record with no study day
  has no rank and is ineligible; among the remaining records,
  `keep: first` selects the smallest rank. A subject with no ranked
  record reads missing in all three selected values.

**Columns:** the chosen donor's decoded term lands in `DSDECOD`, its
representative numeric value lands in `DSVALUE`, and the computed rank is
read through `EOT.EOT_FALLBACK` into `EOT_FALLBACK`. The numbers are
written with the shortest digits that read back as the same value, so
`0.30000000000000004` is not rounded to `0.3`.

**Standard:** ADaM | **Domain:** ADSL
