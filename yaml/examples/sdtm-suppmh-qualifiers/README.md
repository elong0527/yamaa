# SDTM SUPPMH non-standard qualifiers

This focused probe answers one question: how are non-standard qualifier columns
reshaped into SUPPQUAL rows and linked back to their parent record?

## Rule and input boundary

The input is a pre-derived MH slice that already carries `MHSEQ` alongside the
two collected qualifiers the MH domain has no variable for. Two row templates
each emit one SUPPMH record per parent record whose qualifier was collected,
so four parent records produce seven qualifier records: `ECZEMA` has no
medical-record confirmation and contributes only one.

`RDOMAIN`, `IDVAR`, and `QORIG` are literals. `IDVARVAL` is the parent sequence
converted to character, as SUPPQUAL requires. `QEVAL` is `literal: null`
because these values are collected on the CRF rather than assessed, which is
the R005 way to declare an intentionally missing value.

The reshaping mechanism itself is already covered by `../sdtm-lb-findings`.
What this fixture adds is the SUPPQUAL linkage contract and the three gaps
below.

## Status and named gaps

This fixture is a **probe**. It passes as written, but only because the parent
sequence is handed to it as input.

1. **The parent sequence cannot be assigned and consumed in one run.** A
   specification describes one output dataset, so MH and SUPPMH cannot be
   derived together, and `MHSEQ` has to arrive pre-assigned. This is the
   multi-output pipeline gap the plan records for C01 and X10.
2. **The linkage join is not expressible.** If the qualifiers arrived on a
   separate collected dataset, `IDVARVAL` would have to come from MH matched on
   subject plus repeat key. The R003 automatic join uses only output keys that
   also exist on the right side, which here is `[STUDYID, USUBJID]`, and MH is
   not unique on those. `mapping_from` matches one key column to one value
   column and cannot take a compound key. An explicit multi-column equality
   join is required and does not exist.
3. **Referential integrity cannot be verified.** The
   `qualifier-value-has-parent-identifier` check only asserts that the linkage
   columns are populated. Asserting that every `IDVARVAL` exists as an `MHSEQ`
   in MH needs a cross-dataset verification; R009 verifications are row-wise
   over the completed output only.

## Output row order

Rows are appended in row-template order under R001, so the output holds all
`MHFAMHX` records and then all `MHCONF` records. Conventional SUPPQUAL order is
by subject, then parent identifier, then `QNAM`. The two differ, and the
language has no way to reconcile them: `keys` declares identity, column
declaration order controls column layout, and nothing controls output row
order.

The committed expected file uses template order because that is what R001
specifies. If submission-ready row order becomes a requirement, it needs a new
rule and a new field rather than an implementation convention.

## Diagnostics and verifications

No handler path is declared. `IDVARVAL` converts an integer to character under
R005, the same conversion `../sdtm-relrec-related-records` performs, and
`../sdtm-vs-unit-standardization` proposes the rule that governs it.

The exact key is `[STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM]`, and
exactly seven rows are expected. `QVAL` must be present and `Y` or `N`, and
every qualifier value must carry a complete parent identifier.
