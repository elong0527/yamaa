# Repeated Concomitant Medications from ODM

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-cm-odm-repeated.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** create one CM record per reported medication course from long-form
Operational Data Model (ODM) item data.

**Input:** each collected item has its own ODM row. Items belonging to one
medication course share the subject, visit, visit repeat, and form repeat
identifiers. Items from other forms contribute no CM record.

**Variables:**

- `CMTRT` keeps the medication name exactly as reported.
- `CMSTDTC` and `CMENDTC` come from the same medication occurrence; an
  ongoing course has a blank end date.
- `CMROUTE` and `CMINDC` keep that occurrence's route and indication.
- `CMSEQ` orders courses within a subject by visit (screening before
  baseline), visit repeat, and form repeat.

**Note:** A form repeat number reused at a later visit starts a separate
course, and a medication reported in two courses gives two records. A course
without a reported treatment name contributes no CM record.

**Standard:** SDTM | **Domain:** CM
