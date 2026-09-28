# Reject Two Fields That Name One Value

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-schema-ambiguous.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AGE` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item. The export carries the stored value twice, as `Value` and
as `VALUE`, and field names match whatever their case.

**Variables:**

- `AGE` would be the subject's collected age.

Both fields name the stored value, and nothing says which one holds it, so
the input is rejected before any record is read.

**Standard:** SDTM | **Domain:** DM

## How to fix

Keep one field for the stored value. If the second is a vendor's own copy,
drop it from the export or rename it to a name that is not an ODM field,
such as `VALUE_RAW`.
