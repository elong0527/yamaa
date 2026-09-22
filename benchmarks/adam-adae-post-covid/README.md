# Post-COVID Event Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-post-covid.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** mark each Analysis Data Model (ADaM) adverse event record
with `AFTCOVFL` when it starts after the subject's first COVID-19
event.

**Input:** one record per adverse event (AE), carrying the coded term
`AEDECOD` and the event start date `AESTDTC` (kept in the output as
`ASTDT`).

**Variables:**

- `AFTCOVFL`: `Y` for an event ordered strictly after the subject's
  first COVID-19 event: a later start date, or the same start date
  with a higher sequence number. Blank otherwise: the first COVID-19
  event itself, every record of a subject with no COVID-19 event, and
  any record with no start date.

**Note:** ordering is within each subject by start date, with the
sequence number (`AESEQ`) breaking ties on the same date.

**Standard:** ADaM | **Domain:** ADAE
