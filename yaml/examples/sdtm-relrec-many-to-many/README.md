# SDTM RELREC many-to-many relationships

This fixture answers two related questions: how is a RELREC row constructed
from several source datasets, and can a record that participates in more than
one relationship be represented?

## Rule and record grain

Two relationships are collected. `RELID` 1 groups one adverse event with two
concomitant medications, which is the ordinary one-to-many case. `RELID` 2
groups two adverse events with one medication. `HEADACHE` and `ONDANSETRON`
each belong to both, which is what makes the set many-to-many rather than two
independent one-to-many groups. `NAUSEA` carries a single link and `RASH`
carries none, so the one-link and no-link shapes are covered by the same
fixture.

Four row templates produce six RELREC records: one template per link slot per
domain. Records with no link produce nothing.

## Row construction

RELREC has no single base dataset. Each row definition declares its row-driving
`dataset`, and the constructed rows are appended in specification order. Source
record order is preserved within each row definition.

Each row explicitly defines every output variable. `STUDYID` and `USUBJID` are
copied from the row-driving dataset, while `RDOMAIN` names the SDTM domain
containing the related record and does not depend on a dataset alias. The
declared `str` type converts the numeric sequence values to the character
`IDVARVAL` that RELREC requires.

Both source datasets contain a record with no link identifier. The row filters
remove those records, so a missing optional relationship does not create an
incomplete RELREC key.

## A link slot is a column, so the maximum degree is fixed by the schema

Each record can hold two relationships because the collected data has two link
columns and the specification has two row templates per domain. A record in
three relationships would need a third column and a third template in both
domains. The specification therefore encodes the maximum number of
relationships a record may have, which is a property of the data rather than of
the study design.

The natural representation is a separate link table with one row per record and
relationship. That table could be a row driver, and `mapping_from` can now
declare the compound key that reaches `IDVAR` and `IDVARVAL` from the parent
domain, as `../sdtm-suppmh-parent-linkage` does. What is still missing is a way
to state the degree itself rather than one template per slot.

## Group-level relationships cannot be keyed

RELREC also represents a relationship between whole datasets, with `RELTYPE`
carrying `ONE` or `MANY`, `IDVAR` naming a variable, and `IDVARVAL` left
empty. Every row here identifies an individual record, so `RELTYPE` is empty
throughout. R005 requires every value in
`keys` to be non-missing, and `IDVARVAL` is part of the key that makes a
record-level row unique. A dataset-level row therefore cannot be expressed with
this key at all.

The fixture omits group-level rows rather than inventing a placeholder value.
Supporting them needs either a rule permitting a missing key component when
another column distinguishes the row, or a `RELTYPE`-aware key. `RELTYPE` is
declared here as `literal: null` so the column exists in the output and the
gap is visible in the golden file.

## Two further gaps

**Referential integrity is unverifiable.** Nothing checks that `IDVARVAL`
matches an existing `AESEQ` or `CMSEQ`, the same gap
`../sdtm-suppmh-qualifiers` records.

**There is no dedup.** Row templates append. If two link slots on one record
held the same `RELID`, the output would carry a duplicate key and fail, and no
distinct operation exists to prevent it.

## Diagnostics and verifications

No handler path is declared. Rows are appended in row-template order: both AE
slots, then both CM slots. `HEADACHE` appears twice, once per relationship, and
the two rows differ only in `RELID`.

The exact key is `[STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELID]`, and
exactly six rows are expected.
