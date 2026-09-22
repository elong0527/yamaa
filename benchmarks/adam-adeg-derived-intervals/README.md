# Derived ECG Intervals

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adeg-derived-intervals.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add new records for each subject and visit holding the
Bazett-corrected QT interval (`QTCBR`), the Fridericia-corrected QT
interval (`QTCFR`), and the rederived RR duration (`RRR`).

**Input:** collected electrocardiogram (ECG) records with parameter
codes `QT` (QT interval duration), `RR` (time between successive R
waves), and `HR` (heart rate) per subject (`USUBJID`) and visit
(`AVISIT`), carrying the record label (`PARAM`), result (`AVAL`),
and result unit (`AVALU`). QT and RR results use milliseconds
(`ms` or `msec`); heart rate uses beats per minute (`beats/min`).

**Variables:**

- `PARAMCD`: `QTCBR`, `QTCFR`, or `RRR` on the new records; the
  input must not already contain those codes.
- `PARAM`: "QTcB - Bazett's Correction Formula Rederived (ms)",
  "QTcF - Fridericia's Correction Formula Rederived (ms)", or
  "RR Duration Rederived (ms)" on the new records.
- `AVAL`: `QT / SQRT(RR / 1000)` for `QTCBR`,
  `QT / POWER(RR / 1000, 1 / 3)` for `QTCFR`, and `60000` divided
  by the heart rate for `RRR`; for example, a rate of 60 gives 1000.
- `AVALU`: `ms` on the new records.

**Note:** a QTc record is added only when both the QT and the RR
values are present at that visit; an `RRR` record is added only
when the heart rate at that visit is present and not zero.
Collected QT and RR records in units other than `ms` or `msec`
are rejected.

**Standard:** ADaM | **Domain:** ADEG
