# SDTM SUPPMH parent linkage by compound key

This fixture answers one question: how does a derivation reach a parent record
that is identified by more than one column?

## Rule and input boundary

The qualifiers here are collected on their own form rather than alongside the
parent record, which is the arrangement `../sdtm-suppmh-qualifiers` could not
express. Neither the collected form nor MH carries the parent sequence on the
same row, so `IDVARVAL` has to be looked up.

MH is supplied pre-derived. It is unique on `[USUBJID, MHTERM]` and on neither
column alone: subject `CATH-UCSD-0001` reports two conditions, and `ASTHMA` is
reported by two subjects. That makes the compound key load-bearing rather than
decorative. `CATH-UCSD-0002` reports `ASTHMA` as its second condition, so a
lookup that matched on `MHTERM` alone would be ambiguous, and one that matched
on `USUBJID` alone would be ambiguous as well; both would fail R007 uniqueness
rather than return a wrong answer, but the row that would be silently wrong
under a mistaken single-key join is visible in the golden output as `IDVARVAL`
`2` where every other subject's first-listed condition is `1`.

`mapping_from` declares the pair list directly:

```yaml
mapping_from:
  source: [MH_SUPP_RAW.USUBJID, MH_SUPP_RAW.MHTERM]
  dataset: MH
  key: [USUBJID, MHTERM]
  value: MHSEQ
```

R003's automatic join cannot do this. It selects applicable keys from output
`keys` that also exist on the right side, which here is `[STUDYID, USUBJID]`,
and MH is not unique on those. R003's "Declared-key lookup" section states the
distinction: the automatic join derives its keys, `mapping_from` declares them.

Two row templates each emit one SUPPMH record per collected qualifier, so four
collected records produce seven qualifier records: `CATH-UCSD-0001` / `ECZEMA`
has no medical-record confirmation and contributes only one.
`CATH-UCSD-0002` / `HYPERTENSION` is a parent record with no collected
qualifiers at all and contributes none, which is why the base is the collected
form rather than MH.

`RDOMAIN`, `IDVAR`, and `QORIG` are literals. `IDVARVAL` is declared `str` and
the looked-up `MHSEQ` is an integer, so R005 converts it, the same conversion
`../sdtm-suppmh-qualifiers` and `../sdtm-relrec-many-to-many` perform. `QEVAL`
is `literal: null` because these values are collected rather than assessed.

## Status and named gaps

This fixture **passes**. It closes the second gap named in
`../sdtm-suppmh-qualifiers`, which recorded that an explicit multi-column
equality join "is required and does not exist". The remaining two gaps there
are untouched and are not this fixture's subject:

1. **The parent sequence still cannot be assigned and consumed in one run.** MH
   arrives pre-derived because a specification describes one output dataset.
   This fixture removes the need for the *qualifier form* to carry a
   pre-assigned sequence; it does not remove the need for MH to have been
   derived first.
2. **Referential integrity still cannot be verified.** `not_missing` on
   `IDVARVAL` asserts that every collected qualifier found a parent, which is
   most of what matters here and more than the previous fixture could assert.
   It works only because the lookup fails closed: with no `unmapped` handler, a
   qualifier with no parent record is an error rather than a missing value.
   Asserting the converse — that every MH record's qualifiers were collected —
   is a cross-dataset verification and R009 verifications are row-wise over the
   completed output.

A third point is a property of the feature rather than of this fixture.
`source` and `key` pair by position, so transposing one list against the other
is well-formed, passes the length check, and passes the per-position type check
whenever both columns are strings, as both are here. Swapping the two entries
in `key` would join `USUBJID` against `MHTERM`, match nothing, and fail on the
first record. That is fail-closed for this data but not for data where the
transposed pairing happens to match.

## Output row order

Rows are appended in row-template order under R001, so the output holds all
`MHFAMHX` records and then all `MHCONF` records, which is not conventional
SUPPQUAL order. `../sdtm-suppmh-qualifiers` records this as an open question
and nothing here changes it.

## Diagnostics and verifications

No handler path is declared, and that is deliberate: every one of the four
collected qualifier records must match a parent, so `missing` and `unmapped`
are both omitted and both conditions are fatal.

The exact key is `[STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM]`, and
exactly seven rows are expected. `IDVARVAL` must be present, `QVAL` must be
present and `Y` or `N`, `QNAM` must be one of the two declared qualifier names,
and every qualifier value must carry a complete parent identifier.
