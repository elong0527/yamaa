# Build a lab dataset with baseline and change

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-bds.html)

**Goal:** build a laboratory (LB) basic data structure (BDS)
analysis dataset with a baseline flag (`ABLFL`), baseline value
(`BASE`), change (`CHG`), percent change (`PCHG`), and a record
converted to International System of Units (SI) for alanine
aminotransferase (ALT).

**Input:** collected laboratory records for ALT and
aspartate aminotransferase (AST), carrying the collection
date as analysis date (`ADT`) and the numeric result
(`AVAL`) with its unit (`AVALU`), together with each
subject's treatment start date (`TRTSDT`) and actual treatment
(`TRT01A`) from the subject-level analysis dataset (ADSL).

**Variables:**

- `ADT`: the collection date.
- `AVAL`: the collected numeric result, or the result times
  0.0167 on the added SI record.
- `AVALU`: the collected unit, or `ukat/L` on the added SI
  record.
- `ABLFL`: `Y` on the latest record on or before treatment
  start for each subject and parameter; blank otherwise.
- `BASE`: the `AVAL` of the `ABLFL` record, repeated on
  every record for the same subject and parameter.
- `CHG`: `AVAL` minus `BASE`.
- `PCHG`: 100 times `CHG` divided by `BASE`; empty when the
  baseline value is zero, since the percentage is undefined.

**Note:** a collected result with no numeric value produces
no record, neither its own parameter nor any record made
from it. A subject in the subject-level data with no
collected result has no records: the treatment dates enrich
records that already exist and never add one.

**Standard:** ADaM | **Domain:** ADLB
