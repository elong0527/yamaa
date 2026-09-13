# Bazett-corrected QT interval

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adeg-bazett-qtc.html)

**Goal:** add a new record for each subject and visit holding the
Bazett-corrected QT interval (QTcB) under the code `QTCBR`,
computed from the QT and RR interval records collected at that
visit.

**Input:** collected electrocardiogram (ECG) records with parameter
codes `QT` (QT interval duration) and `RR` (time between successive
R waves) per subject (`USUBJID`) and visit (`AVISIT`), carrying the
record label (`PARAM`), result (`AVAL`), and result unit (`AVALU`).
Input results use milliseconds (`ms` or `msec`).

**Variables:**

- `PARAMCD`: `QTCBR` on the new records; the input must not already
  contain that code.
- `PARAM`: "QTcB - Bazett's Correction Formula Rederived (ms)" on
  the new records.
- `AVAL`: the QT value divided by the square root of the RR value
  in seconds (`QT / SQRT(RR / 1000)`).
- `AVALU`: `ms` on the new records.

**Note:** a new record is added only when both the QT and the RR
values are present at that visit; a missing value, or a visit with
no QT or no RR record, yields no new record. Collected QT and RR
records in units other than `ms` or `msec` are rejected.

**Standard:** ADaM | **Domain:** ADEG
