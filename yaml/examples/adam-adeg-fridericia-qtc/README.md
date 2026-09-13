# Add a Fridericia-corrected QT record per analysis visit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adeg-fridericia-qtc.html)

**Goal:** add a Fridericia-corrected QT (QTcF) record, coded
`QTCFR`, at each subject and analysis visit with both a QT
and an RR interval, computed from the two intervals collected
at the same visit.

**Input:** electrocardiogram records carrying parameter code
(`PARAMCD`), parameter name (`PARAM`), analysis visit
(`AVISIT`), collected result (`AVAL`), and result unit
(`AVALU`), including heart rate (`HR`), QT duration (`QT`),
and RR duration (`RR`) records.

**Variables:**

- `PARAMCD`: the source code, or `QTCFR` on the added record.
- `PARAM`: the source name, or the QTcF name on the added
  record.
- `AVAL`: the source result, or the QT interval in milliseconds
  divided by the cube root of the RR interval converted to
  seconds on the added record;
  for example, a QT of 400 milliseconds (ms) with an RR of 512
  ms gives 500 ms.
- `AVALU`: the source unit, or `ms` on the added record.

**Note:** the added record needs both a QT and an RR record at
the same subject and visit; when either is missing, no record
is added and the collected records stay unchanged. The formula
and parameter identity follow
[`pharmaverse/admiral`](https://github.com/pharmaverse/admiral)
commit `e32e5689d7fd03e224ddbcfc369c332c5df837d9`,
`R/derive_param_qtc.R`.

**Standard:** ADaM | **Domain:** ADEG
