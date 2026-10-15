# Repeated Adverse Events from ODM

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-odm-repeated.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** create one adverse event (AE) record per reported event from
long-form Operational Data Model (ODM) item data.

**Input:** each collected item has its own ODM row. Items belonging to one
event share the subject, visit, visit repeat, form, and form repeat
identifiers.

**Variables:**

- `AESEQ` numbers the subject's events from 1 in visit order (screening,
  then baseline), then by visit repeat and form repeat; an event at any
  other visit stops the run.
- `AETERM` keeps the free-text term exactly as reported.
- `AESTDTC` and `AEENDTC` come from the same event occurrence; a missing end
  date stays blank.
- `AESEV` and `AESER` come from that occurrence's severity and seriousness
  items.

**Note:** each form occurrence is its own event, so a form repeat number
reused at a later visit, or the same term reported in two occurrences,
still gives separate records. Only AE forms are read, and an occurrence
without a reported term contributes no AE record.

**Standard:** SDTM | **Domain:** AE
