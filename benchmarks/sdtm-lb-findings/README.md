# Build One Record per Collected Lab Result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-lb-findings.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per collected calcium and creatinine
result with `LBTESTCD`, `LBTEST`, `LBORRES`, `LBORRESU`,
`LBSTRESN`, `LBSTRESU`, `LBSTAT` and `LBDTC`.

**Input:** long-form Operational Data Model (ODM) rows, one row
per collected item, grouped by study, subject and visit. Each
visit group carries a collection-date item plus one calcium item
and one creatinine item, whichever were collected.

**Variables:**

- `LBSEQ` numbers the records within a subject by collection
  date, then test code.
- `LBTESTCD` is `CA` for calcium rows and `CREAT` for creatinine
  rows.
- `LBTEST` is `Calcium` when the test code is `CA` and
  `Creatinine` when it is `CREAT`.
- `LBORRES` is the collected entry, kept exactly as reported;
  empty when the test was not done.
- `LBORRESU` is `mg/dL` for every record with a result; empty when
  the test was not done.
- `LBSTRESN` is the numeric form of the collected entry; missing
  when the entry is text rather than a number, or when the test
  was not done.
- `LBSTRESU` is `mg/dL` for every record with a result; empty when
  the test was not done.
- `LBSTAT` is `NOT DONE` when the collected entry says the test
  was not done; blank otherwise.
- `LBDTC` is the collection date from the same visit group.

**Note:** a test with no collected entry produces no record, so
the records are only results actually reported. A `NOT DONE`
entry does produce a record: the result and units stay
empty and `LBSTAT` carries `NOT DONE`.

**Standard:** SDTM | **Domain:** LB
