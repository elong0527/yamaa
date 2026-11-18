# Reject Dose Expansion

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

## How to fix

Count the administrations from the collected record itself. A relation
that writes `repeat` builds one record for each administration a
collected record counts, whatever the count, so no administration is
written out in advance, as `schema-lookup-repeat-rows` shows for a
count taken from a record's dates:

```yaml
intermediates:
  - id: COUNTED
    dataset: EX
    filter: "EX.EXDOSCNT IS NOT NULL"
  - id: ADMIN
    dataset: COUNTED
    repeat: {count: COUNTED.EXDOSCNT, index: N}
    derivations:
      STUDYID: COUNTED.STUDYID
      USUBJID: COUNTED.USUBJID
      EXSEQ: COUNTED.EXSEQ
      NDOSE: COUNTED.EXDOSCNT
      ADOSEN: N
```

A row template that reads `ADMIN` then builds one row for each of
them. A record with a zero count builds none. A record with no count
rejects the run unless it is left out first, as the filter here does:
decide that such a record stands for no administration before
writing that filter.

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
