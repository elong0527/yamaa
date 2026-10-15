# Reference Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-dates.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per enrolled subject carrying informed
consent date (`RFICDTC`), first and last exposure dates (`RFXSTDTC`,
`RFXENDTC`), reference start date (`RFSTDTC`), last participation
date (`RFPENDTC`), and reference end date (`RFENDTC`).

**Input:** collected demographics rows with consent date, exposure
rows with start and end dates and sequence number, disposition rows
with category and start date, and adverse event rows with end date.

**Variables:**

- `RFICDTC` is the consent date as collected; it is required, so a
  subject with no consent date stops the run.
- `RFXSTDTC` is the earliest exposure start date; missing when no
  exposure row carries a start date.
- `RFXENDTC` is the latest exposure end date; missing when no exposure
  row carries an end date.
- `RFSTDTC` is the reference start date, taken here as the first
  exposure date, so it is missing whenever `RFXSTDTC` is.
- `RFPENDTC` is the last date the subject is known to have
  participated: the latest of the last exposure end date, the latest
  disposition event date, and the latest adverse event end date. A
  missing one is skipped; it is missing only when all three are.
- `RFENDTC` is the reference end date, the end of the subject's
  reference period. It is missing whenever `RFSTDTC` is missing, so
  a subject who never entered the reference period has no end date.

**Note:** exposure, disposition, and adverse event rows count only for
the subject with the same study and subject identifiers, so the same
subject identifier in two studies gives two independent records, and a
subject with no exposure has missing exposure, reference start, and
reference end dates.

**Standard:** SDTM | **Domain:** DM
