# Declare value-level metadata for a findings domain

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-value-level-metadata.html) [![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** draft - first commit.

**Goal:** build one LB record per collected lab result, declaring the
original units per test code: glucose results carry `CL_UNIT_GLUCOSE`
and hemoglobin results carry `CL_UNIT_HGB`.

**Input:** one row per collected lab result carrying the test code,
test name, result, and original units.

**Variables:**

- `DOMAIN` is fixed to `LB`.
- `STUDYID` is the study identifier.
- `USUBJID` is the unique subject identifier.
- `LBTESTCD` is the collected lab test short name.
- `LBTEST` is the collected lab test name.
- `LBORRES` is the collected result in original units.
- `LBORRESU` is the collected original units, with value-level
  metadata: the `GLUC` entry binds `CL_UNIT_GLUCOSE` and the `HGB`
  entry binds `CL_UNIT_HGB`.

**Note:** `LBORRESU` carries `values` inside its `submission` block:
each entry names one `LBTESTCD` literal and overrides the
column's submission fields for that value, inheriting everything else.
The declaration is data-independent and annotation-only; define.xml
value-level generation is deferred and reported loudly rather
than silently dropped.

**Standard:** SDTM | **Domain:** LB
