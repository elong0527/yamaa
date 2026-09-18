# Build one record per collected result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-findings.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** build one record per collected calcium and creatinine
result with `LBTESTCD`, `LBTEST`, `LBORRES`, `LBORRESU`,
`LBSTRESN`, `LBSTRESU` and `LBDTC`.

**Input:** long-form Operational Data Model (ODM) rows, one row
per collected item, with the collected entry in the value field.
Calcium rows carry one item, creatinine rows carry another, and
the collection date rides along in the same visit group.

**Variables:**

- `LBTESTCD` is `CA` for calcium rows and `CREAT` for creatinine
  rows.
- `LBTEST` is `Calcium` when the test code is `CA` and
  `Creatinine` when it is `CREAT`.
- `LBORRES` is the collected entry, kept exactly as reported,
  including text such as `NOT DONE`.
- `LBORRESU` is `mg/dL` for every record.
- `LBSTRESN` is the numeric form of the collected entry; missing
  when the entry is text rather than a number, such as `NOT DONE`.
- `LBSTRESU` is `mg/dL` for every record.
- `LBDTC` is the collection date from the same visit group.

**Note:** a test with no collected entry produces no record, so the
records are only results actually reported.

**Standard:** SDTM | **Domain:** LB
