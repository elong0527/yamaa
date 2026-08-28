# SDTM RELREC many-to-many relationships

This focused probe answers one question: can a record that participates in more
than one relationship be represented?

`../sdtm-relrec-related-records` covers the one-to-many case, where each record
carries at most one link identifier. This fixture keeps the same structure and
adds records that belong to two relationships.

## Rule and record grain

Two relationships are collected. `RELID` 1 groups one adverse event with two
concomitant medications. `RELID` 2 groups two adverse events with one
medication. `HEADACHE` and `ONDANSETRON` each belong to both, which is what
makes the set many-to-many rather than two independent one-to-many groups.

Four row templates produce six RELREC records: one template per link slot per
domain. Records with no link produce nothing.

## A link slot is a column, so the maximum degree is fixed by the schema

Each record can hold two relationships because the collected data has two link
columns and the specification has two row templates per domain. A record in
three relationships would need a third column and a third template in both
domains. The specification therefore encodes the maximum number of
relationships a record may have, which is a property of the data rather than of
the study design.

The natural representation is a separate link table with one row per record and
relationship. That table could be a row driver, but the RELREC row also needs
`IDVAR` and `IDVARVAL` describing which domain variable identifies the parent,
and those come from the parent domain. Reaching them would need the
multi-column equality join recorded by `../sdtm-suppmh-qualifiers`.

## Group-level relationships cannot be keyed

RELREC also represents a relationship between whole datasets, with `IDVAR`
naming a variable and `IDVARVAL` left empty. R005 requires every value in
`keys` to be non-missing, and `IDVARVAL` is part of the key that makes a
record-level row unique. A dataset-level row therefore cannot be expressed with
this key at all.

The fixture omits group-level rows rather than inventing a placeholder value.
Supporting them needs either a rule permitting a missing key component when
another column distinguishes the row, or a `RELTYPE`-aware key. `RELTYPE` is
declared here as `literal: null` so the column exists in the output and the
gap is visible in the golden file.

## Status and named gaps

This fixture is a **probe**. It names four gaps.

1. **Relationship degree is fixed in the specification.** One row template per
   link slot means the YAML grows with the data rather than the design.
2. **Group-level rows cannot satisfy the key contract.** A missing `IDVARVAL`
   is legitimate in RELREC and illegal as a key value.
3. **Referential integrity is unverifiable.** Nothing checks that `IDVARVAL`
   matches an existing `AESEQ` or `CMSEQ`, the same gap
   `../sdtm-suppmh-qualifiers` records.
4. **There is no dedup.** Row templates append. If two link slots on one record
   held the same `RELID`, the output would carry a duplicate key and fail, and
   no distinct operation exists to prevent it.

## Diagnostics and verifications

No handler path is declared. Rows are appended in row-template order: both AE
slots, then both CM slots. `HEADACHE` appears twice, once per relationship, and
the two rows differ only in `RELID`.

The exact key is `[STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELID]`, and
exactly six rows are expected.
