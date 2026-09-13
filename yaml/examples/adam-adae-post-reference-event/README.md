# Flag events after the first COVID-19 event

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-post-reference-event.html)

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
