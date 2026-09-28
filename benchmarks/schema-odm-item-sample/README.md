# Lab Results with Their Own Sample Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-odm-item-sample.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive one laboratory record per collected result, dated by the
sample the result came from.

**Input:** long-form Operational Data Model (ODM) data as an EDC vendor
exports it, one row per collected item. The vendor writes every field name
in capitals and adds a site and a user field. Each lab sample is one repeat
of the lab item group: a collection date and its results.

**Variables:**

- `LBSEQ` numbers each subject's results by collection date, then test.
- `VISIT` is the visit the sample was collected at.
- `LBTESTCD` is the test of the result item, `ALT` or `AST`.
- `LBORRES` is the result as collected; an empty result stays missing.
- `LBDTC` is the collection date of the result's own sample, never of
  another sample at the same visit; a sample with no date keeps a missing
  date.

**Note:** field names match whatever their case, and the vendor's own
fields take no part. A date recorded twice for one sample stops the run.

**Standard:** SDTM | **Domain:** LB
