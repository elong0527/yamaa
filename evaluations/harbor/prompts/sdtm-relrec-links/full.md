Following CDISC SDTM standards, use the provided AE and CM datasets to create a
RELREC dataset with one record per relationship participation.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

AE records carry up to two link identifiers, in AELNKID1 and AELNKID2,
and CM records likewise in CMLNKID1 and CMLNKID2. A record with no link
identifier contributes no row. A record with one link identifier
contributes one row, and a record with two different link identifiers
contributes two rows, one for each identifier. RDOMAIN is "AE" on a row
from an AE record and "CM" on a row from a CM record. IDVAR is "AESEQ"
on a row from an AE record and "CMSEQ" on a row from a CM record.
IDVARVAL is the sequence number of the record the row points at, as
text, and every row carries one. RELTYPE has no value on any row,
because each row points at one record rather than a whole dataset.
RELID is the link identifier of the relationship the row takes part in:
rows sharing a RELID are related to one another, and every row carries
one. Link identifiers are per subject, so the same RELID under another
subject names a different relationship.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
