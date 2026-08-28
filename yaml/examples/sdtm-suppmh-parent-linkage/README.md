# SDTM SUPPMH parent linkage by compound key

This fixture answers one question: how does a derivation reach a parent record
that is identified by more than one column?

## Rule and input boundary

The qualifiers are collected on their own form rather than alongside the parent
record, so neither dataset carries the parent sequence on the same row and
`IDVARVAL` has to be looked up. `mapping_from` declares the pair list directly:

```yaml
mapping_from:
  source: [MH_SUPP_RAW.USUBJID, MH_SUPP_RAW.MHTERM]
  dataset: MH
  key: [USUBJID, MHTERM]
  value: MHSEQ
```

MH is supplied pre-derived. It is unique on `[USUBJID, MHTERM]` and on neither
column alone: `CATH-UCSD-0001` reports two conditions, and `ASTHMA` is reported
by two subjects. `CATH-UCSD-0002` reports `ASTHMA` as its *second* condition,
so a mistaken single-key join is visible in the golden output as `IDVARVAL` `2`
rather than silently agreeing.

R003's automatic join cannot do this. It selects applicable keys from output
`keys` that also exist on the right side, which here is `[STUDYID, USUBJID]`,
and MH is not unique on those. R003's "Declared-key lookup" section states the
distinction: the automatic join derives its keys, `mapping_from` declares them.

Two row templates each emit one SUPPMH record per collected qualifier, so four
collected records produce seven qualifier records: `CATH-UCSD-0001` / `ECZEMA`
has no medical-record confirmation and contributes only one.
`CATH-UCSD-0002` / `HYPERTENSION` is a parent record with no collected
qualifiers and contributes none, which is why the base is the collected form
rather than MH.

`RDOMAIN`, `IDVAR`, and `QORIG` are literals. `IDVARVAL` is declared `str` and
the looked-up `MHSEQ` is an integer, so R005 converts it. `QEVAL` is
`literal: null` because these values are collected rather than assessed.

## Two gaps this fixture names

**Positional pairing cannot catch a transposition.** `source` and `key` pair by
position, so swapping the two entries in `key` is well-formed, passes the
length check, and passes the per-position type check whenever both columns are
strings, as both are here. It would join `USUBJID` against `MHTERM` and fail on
the first record — fail-closed for this data, but not for data where the
transposed pairing happens to match.

**Referential integrity is still unverifiable.** `not_missing` on `IDVARVAL`
asserts that every collected qualifier found a parent, and works only because
the lookup fails closed: with no `unmapped` handler, a qualifier with no parent
is an error rather than a missing value. The converse — that every MH record's
qualifiers were collected — is a cross-dataset verification, and R009
verifications are row-wise over the completed output.

## Diagnostics and verifications

No handler path is declared, and that is deliberate: every collected qualifier
record must match a parent, so `missing` and `unmapped` are both omitted and
both conditions are fatal.

Rows are appended in row-template order under R001, so the output holds all
`MHFAMHX` records and then all `MHCONF` records, which is not conventional
SUPPQUAL order; `../sdtm-suppmh-qualifiers` records that as an open question.
The exact key is `[STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM]`, and
exactly seven rows are expected. `IDVARVAL` must be present, `QVAL` must be
present and `Y` or `N`, `QNAM` must be one of the two declared qualifier names,
and every qualifier value must carry a complete parent identifier.
