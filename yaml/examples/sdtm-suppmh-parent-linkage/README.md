# SDTM SUPPMH: link qualifiers collected on their own form to a parent record

This example uses a pre-derived medical-history domain with a separately
collected qualifier form, and a `yamaa` specification to derive one
supplemental record per collected qualifier:

- `RDOMAIN`, `IDVAR`, and `IDVARVAL` point back at the medical-history record
  the qualifier belongs to. Neither form carries that record's sequence number
  alongside the qualifier, so it is found by matching on the subject together
  with the reported condition. Neither alone identifies a record: a subject may
  report several conditions and a condition may be reported by several
  subjects;
- `QNAM` and `QLABEL` name the qualifier and `QVAL` carries its value;
- `QORIG` records that the value came from the case report form, and `QEVAL` is
  empty because a collected value is not an assessment.

A qualifier that finds no medical-history record is an error rather than a
record with an empty link, so every supplemental record here points at a real
parent. The reverse is not checked: a medical-history record whose qualifiers
were never collected simply contributes nothing.

Records are grouped by qualifier rather than by subject.
