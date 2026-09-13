# Reject an aggregate dose expanded past its written rows

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adex-single-dose-expansion.html)

**Goal:** build an administration record for each administration
in a collected exposure (EX) record, carrying the collected
record's sequence number and `EXTRT`, `EXDOSE`, `EXDOSU`,
`ASTDT`, `AENDT`, and `NDOSE` through and numbering each
administration in `ADOSEN`.

**Input:** collected exposure records with treatment, dose, unit,
start and end dates, and a count of administrations: `EXTRT`,
`EXDOSE`, `EXDOSU`, `EXSTDTC`, `EXENDTC`, and `EXDOSCNT`.

**Variables:**

- `EXTRT` is the treatment given, carried through from `EXTRT`
  in the exposure input.
- `EXDOSE` is the amount given at each administration, carried
  through from `EXDOSE` in the exposure input.
- `EXDOSU` is the unit the amount is measured in, carried through
  from `EXDOSU` in the exposure input.
- `ASTDT` is the first day the collected record covers, taken
  from `EXSTDTC` in the exposure input and repeated on every
  administration built from that record.
- `AENDT` is the last day the collected record covers, taken from
  `EXENDTC` in the exposure input and repeated on every
  administration built from that record.
- `ADOSEN` numbers the administrations built from one collected
  record from one upward: the first takes value `1` when
  `EXDOSCNT` is at least `1`, the second takes value `2` when
  `EXDOSCNT` is at least `2`, and the third takes value `3` when
  `EXDOSCNT` is at least `3`.
- `NDOSE` is how many administrations the collected record stands
  for, taken from `EXDOSCNT` in the exposure input.

Only three administrations are written out in advance, so a
collected record standing for more administrations loses the ones
past the third. A declared check compares the largest built
`ADOSEN` against `NDOSE`, and a record holding more
administrations than were built rejects the run, so no artifact
is accepted. The expected output records the completed dataset
presented to the failing check.

**Note:** administrations built from one collected record share
that record's treatment, dose, unit, and dates; only `ADOSEN`
differs between them.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

The administration grain belongs in the input data. Expand the
aggregate record into one collected record for each administration
upstream, and read those records here one to one. For
expected-but-uncollected rows, use the long-form planning input in
`adam-advs-once-measured-carry-forward` and enrich it from collected
data.

If the analysis genuinely needs only the totals, drop the
administration grain and key on the collected record instead:

```yaml
keys: [STUDYID, USUBJID, EXSEQ]
```

Do not keep the grain and widen the written-out administrations to
whatever the current extract needs. It answers correctly only for data
that has already been seen, and the next extract with a longer record
loses administrations again without warning.
