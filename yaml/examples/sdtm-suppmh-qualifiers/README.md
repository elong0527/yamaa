# SDTM SUPPMH: reshape extra qualifiers into supplemental records

This example uses a pre-derived medical-history slice and a `yamaa`
specification to derive one supplemental record per collected qualifier:

- `RDOMAIN`, `IDVAR`, and `IDVARVAL` point back at the medical-history record
  the qualifier belongs to, naming the domain, the variable that identifies a
  record there, and that record's sequence number as text;
- `QNAM` and `QLABEL` name the qualifier, and `QVAL` carries its collected
  value;
- `QORIG` records that the value came from the case report form, and `QEVAL` is
  empty because a collected value is not an assessment.

Each medical-history record contributes one supplemental record per qualifier
that was actually collected, so a record with only one of the two qualifiers
contributes only one. The records are presented in submission order: by
subject, then by the medical-history record they point at, then by qualifier
name.
