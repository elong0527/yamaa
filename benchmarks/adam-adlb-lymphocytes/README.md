# Derive absolute differential records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-lymphocytes.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** add absolute differential records (`LYMPH`, `NEUT`,
`MONO`, `EOS`, `BASO`) whose analysis value (`AVAL`) is the
white blood cell count times the matching differential fraction,
and flag them with the record-type flag (`DTYPE`).

**Input:** SDTM laboratory records (`LB`) carrying the subject
identifier (`USUBJID`), visit (`VISIT`), test code (`LBTESTCD`),
test name (`LBTEST`), and numeric result (`LBSTRESN`): white blood
cell counts (`WBC`), differential fractions (`LYMLE`, `NEUTLE`,
`MONOLE`, `EOSLE`, `BASOLE`), a few lab-reported absolute
differentials, and unrelated parameters.

**Variables:**

- `AVAL`: the collected result on a collected record; on an
  added absolute record, the `WBC` count times the matching
  differential fraction for the same subject and visit.
- `DTYPE`: `CALCULATION` on an added absolute record; empty on
  a collected record.

**Note:** one record is added per subject, visit, and
differential: only when both a `WBC` count and that differential's
fraction are present for the subject and visit, and no record of
the new absolute parameter is already there. A differential with
a fraction but no `WBC` count gains nothing, and an existing
absolute record blocks only its own differential. Each
contributing parameter may appear at most once within a subject
and visit; a subject and visit with repeated `WBC` or fraction
results stops the run instead of using a value picked by amount
or position.

**Standard:** ADaM | **Domain:** ADLB
