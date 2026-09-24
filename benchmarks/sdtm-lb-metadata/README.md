# Value-Level Metadata

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-metadata.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per collected glucose and creatinine
result, with per-test submission metadata: each `LBTESTCD` value
gets its own codelist and origin for `LBORRESU`, overriding the
shared column-level declaration.

**Input:** long-form Operational Data Model (ODM) rows, one row
per collected item, with the collected entry in the value field.
Glucose rows carry one item, creatinine rows carry another, and
the collection date rides along in the same visit group.

**Variables:**

- `LBTESTCD` is `GLUC` for glucose rows and `CREAT` for
  creatinine rows.
- `LBTEST` is `Glucose` when the test code is `GLUC` and
  `Creatinine` when it is `CREAT`.
- `LBORRES` is the collected entry, kept exactly as reported.
- `LBORRESU` is `mg/dL` for every record. Its submission
  metadata is stated once for the whole column and overridden for
  each test code: glucose records name the glucose codelist with a
  vendor origin, creatinine records the creatinine codelist with an
  investigator origin.
- `LBSTRESN` is the numeric form of the collected entry; missing
  when the entry is not a number.
- `LBSTRESU` is `mg/dL` for every record.
- `LBDTC` is the collection date from the same visit group.

**Note:** a test with no collected entry produces no record, so the
records are only results actually reported. The per-value
submission metadata is validated when the specification loads;
the engine output is unaffected by it.

**Standard:** SDTM | **Domain:** LB
