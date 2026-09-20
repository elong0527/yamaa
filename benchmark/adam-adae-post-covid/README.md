# Post-COVID Event Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-post-covid.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** mark each Analysis Data Model (ADaM) adverse event record
with `AFTCOVFL` when it occurs after the subject's first COVID-19
event.

**Input:** one record per adverse event (AE) carrying the coded term
`AEDECOD` and the analysis start date `ASTDT`; the first COVID-19
event is the earliest record whose coded term is `COVID-19`.

**Variables:**

- `AFTCOVFL`: `Y` for an event ordered strictly after the subject's
  first COVID-19 event; blank otherwise, including the first COVID-19
  event itself and every record of a subject with no COVID-19 event.

**Note:** ordering is within each subject by start date, with the
sequence number (`AESEQ`) breaking ties on the same date.

**Standard:** ADaM | **Domain:** ADAE
