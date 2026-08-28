# SDTM LB multi-form consolidation

This standalone ODM-to-SDTM fixture condenses the CATH serum, skin-biopsy,
saliva, and tape-strip sources into one LB dataset. It uses the real CATH
study, event, item-group, and item OIDs with synthetic values and dates chosen
to make edge behavior visible.

## Status and boundary

The specification uses only registered schema constructs, but portable ODM
contextual binding is still draft under R002. In particular, the exact context
keys must include event and item-group repeat keys for the repeated unscheduled
saliva collections to resolve the correct date.

The fixture covers only ODM to SDTM. The existing `../adam-adlb-bds` fixture is
the separate SDTM-to-ADaM boundary.

## Business rule and record grain

Each non-missing result item emits one LB record. Six row templates consolidate
four ODM forms:

- serum 25-hydroxyvitamin D from `IG.NCT00789880.LB`;
- lesional and non-lesional IL-13 mRNA from `IG.NCT00789880.BX`;
- saliva cathelicidin protein from `IG.NCT00789880.SAL`;
- lesional and non-lesional tape-strip cathelicidin protein from
  `IG.NCT00789880.TS`.

Each form supplies its own contextual collection-date item. Result records
carry form-specific test, category, specimen, location, and unit metadata.
`VISIT` and `VISITNUM` are mapped from the ODM event OID. `LBSEQ` is assigned
only after all six templates have been appended.

## Challenge cases

The two-subject input deliberately includes:

- a Non-AD subject with no lesional biopsy or tape-strip items, representing
  structural inapplicability rather than a collected missing value;
- explicit blank saliva and tape-strip result items, neither of which emits a
  phantom LB record;
- numeric zero results, which are valid observations and must be retained;
- a collected saliva result of `NOT DONE`, retained in `LBORRES`/`LBSTRESC`,
  converted to missing `LBSTRESN`, and identified by `LBSTAT`;
- two unscheduled saliva item groups with different dates, requiring the
  item-group repeat key to resolve the correct contextual date;
- same-day lesional and non-lesional tape-strip results whose `LBSEQ` sort
  values tie completely, requiring row-template order as the tie-breaker;
- the same `CAMPPRO` test code from saliva and tape-strip sources, distinguished
  by specimen and location metadata;
- declaration order that differs from chronological `LBSEQ` order.

Structural absence and an explicit blank both suppress output under the
current row filters. That is intentional for this fixture, but it exposes a
future diagnostics question: validation may need to distinguish an
inapplicable item from a present-but-missing collection.

The six templates repeat large blocks of near-identical metadata. That
repetition is a finding, not a style choice: the language has no reusable
Findings template, and the fixture does not invent a macro syntax to hide it.

## Deterministic output

Rows remain in row-template order: serum, biopsy lesional, biopsy
non-lesional, saliva, tape-strip lesional, then tape-strip non-lesional. Within
each template, ODM base-record order is retained. `LBSEQ` sorts within subject
by `LBDTC`, `LBTESTCD`, and `LBSPEC`; complete ties retain template order and
then base-record order. Sequence assignment does not reorder rows. `LBLOC` is
not a sort variable because it is intentionally missing for serum and saliva,
and portable missing-value ordering is not defined.

The exact output key is `[STUDYID, USUBJID, LBSEQ]`. Exactly 18 LB records are
expected: 12 for `CATH-UCSD-0001` and 6 for `CATH-UCSD-0002`.

## Diagnostics and verifications

`columns[name=LBSTRESN].derivation.conversion_failure` has expected count one,
for the collected `NOT DONE` saliva result. No other handler path is declared.

The fixture expects all column verifications to pass, unique output keys, an
exact row count of 18, and two named implication checks relating `LBORRES`,
`LBSTRESN`, and `LBSTAT` to pass for every row.
