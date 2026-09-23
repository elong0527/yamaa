# Lab BDS Build

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-bds.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build a laboratory (LB) basic data structure (BDS)
analysis dataset for alanine aminotransferase (ALT) and
aspartate aminotransferase (AST): flag each subject's baseline
record (`ABLFL`), carry its value (`BASE`), compute change
(`CHG`) and percent change (`PCHG`), and add one ALT record
converted to International System of Units (SI) (`ALTSI`),
alongside each subject's treatment start date (`TRTSDT`) and
actual treatment (`TRT01A`).

**Input:** collected ALT and AST results with collection dates,
numeric results, and units, plus subject-level treatment start
dates and actual treatments.

**Variables:**

- `ADT` is the collection date of the result.
- `TRTSDT` is the subject's treatment start date.
- `TRT01A` is the subject's actual treatment.
- `AVAL` is the collected numeric result, or the result times
  0.0167 on the SI record.
- `AVALU` is the collected unit, or `ukat/L` on the SI record.
- `ABLFL` is `Y` on the latest record on or before the
  treatment start date for each subject and parameter; blank
  otherwise.
- `BASE` is the `AVAL` of the flagged baseline record,
  repeated on every record for the same subject and parameter.
- `CHG` is `AVAL` minus `BASE`.
- `PCHG` is 100 times `CHG` divided by `BASE`; empty when the
  baseline value is zero, since the percentage is undefined.

**Note:** a collected result with no numeric value produces no
record. A subject with no collected results has no records: the
treatment dates enrich records that already exist and never add
one. A subject with a single collected result still gets a
baseline flag, and a result collected on the treatment start
date itself counts as baseline.

**Standard:** ADaM | **Domain:** ADLB
