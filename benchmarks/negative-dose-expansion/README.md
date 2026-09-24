# Reject Dose Expansion

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-dose-expansion.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build an administration record for each administration
in a collected exposure (EX) record, carrying the collected
record's sequence number and `EXTRT`, `EXDOSE`, `EXDOSU`,
`ASTDT`, `AENDT`, and `NDOSE` through and numbering each
administration in `ADOSEN`.

**Input:** collected exposure records with treatment, dose, unit,
start and end dates, and a count of administrations: `EXTRT`,
`EXDOSE`, `EXDOSU`, `EXSTDTC`, `EXENDTC`, and `EXDOSCNT`.

**Variables:**

- `EXDOSE` is the amount given at each administration.
- `ASTDT` is the first day the collected record covers, taken
  from `EXSTDTC`.
- `AENDT` is the last day the collected record covers, taken from
  `EXENDTC`.
- `ADOSEN` numbers the administrations built from one collected
  record, from `1` up to its count (`EXDOSCNT`). Only three
  administrations are written out in advance, so a record standing
  for more loses the ones past the third, and a record with a
  missing or zero count builds none.
- `NDOSE` is how many administrations the collected record stands
  for, taken from `EXDOSCNT`. A record whose `NDOSE` is larger than
  the highest `ADOSEN` built from it rejects the run, so no artifact
  is accepted. The expected output records the completed dataset
  presented to that failing check.

**Note:** administrations built from one collected record share
that record's treatment, dose, unit, and dates; only `ADOSEN`
differs between them.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

The per-administration rows belong in the input data. Expand the
aggregate record into one collected record for each administration
upstream, and read those records here one to one. For
expected-but-uncollected rows, use the long-form planning input in
`adam-advs-carryforward` and enrich it from collected
data.

If the analysis genuinely needs only the totals, drop the administration level
and key on the collected record instead:

```yaml
keys: [STUDYID, USUBJID, EXSEQ]
```

Do not keep the administration level and widen the written-out
administrations to whatever the current extract needs. It answers
correctly only for data that has already been seen, and the next
extract with a longer record loses administrations again without
warning.
