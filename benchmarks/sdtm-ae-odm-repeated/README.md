# Repeated Adverse Events from ODM

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-ae-odm-repeated.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** create one AE record per reported adverse event from long-form ODM
item data.

**Input:** each collected item has its own ODM row. Items belonging to one
event share the subject, visit, and form repeat identifiers. The extract
includes a non-AE item and an incomplete repeated group to exercise record
selection.

**Variables:**

- `AETERM` keeps the free-text term exactly as reported.
- `AESTDTC` and `AEENDTC` come from the same event occurrence; a missing end
  date stays blank.
- `AESEV` and `AESER` come from that occurrence's severity and seriousness
  items.
- `AESEQ` orders events within a subject by visit and event repeat.

**Note:** Two events at screening and a later event with a reused form
repeat number remain distinct. The two Headache entries remain separate
events. A repeated group without a reported term contributes no AE record.

**Standard:** SDTM | **Domain:** AE
