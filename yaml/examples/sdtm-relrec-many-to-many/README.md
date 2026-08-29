# SDTM RELREC: record relationships between events and medications

This example uses collected adverse-event and concomitant-medication data and a
`yamaa` specification to derive one record per participation in a relationship:

- `RDOMAIN`, `IDVAR`, and `IDVARVAL` identify the related record: the domain it
  lives in, the variable that identifies a record there, and that record's
  sequence number as text;
- `RELID` names the relationship the record takes part in. Records sharing a
  `RELID` are related to one another;
- `RELTYPE` is empty throughout, because every record here identifies an
  individual record rather than a whole dataset.

A record may take part in more than one relationship and contributes one output
record for each, so an event linked to two relationships appears twice,
differing only in `RELID`. A record in no relationship contributes nothing.

How many relationships a record can join is fixed by how many link fields the
collection carries, so a record in a third relationship would need the
collected data and the specification to grow together.
