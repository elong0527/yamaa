Following CDISC SDTM standards, use the provided AE and CM datasets to
create a RELREC dataset with one row per relationship participation.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

RDOMAIN names the related domain, and IDVAR its sequence variable (AESEQ
on AE rows, CMSEQ on CM rows). IDVARVAL is the sequence number of the
related record as text, and is always present. RELTYPE has no value
throughout, because each row points at one record rather than a whole
dataset. RELID names the relationship the row takes part in; rows sharing
a value are related to one another, and every row carries one.

A record with no link identifier contributes no row, while a record naming
two link identifiers contributes one row per identifier. Link numbers are
per-subject: USUBJID keeps a reused number in distinct relationships.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
