Following CDISC SDTM standards, use the provided AE and CM datasets to
create a RELREC dataset with one row per relationship participation.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

RDOMAIN names the related domain, and IDVAR its sequence variable (AESEQ on AE
rows, CMSEQ on CM rows). IDVARVAL is the sequence number of the related record
as text. RELTYPE has no value throughout. RELID is the collected link number,
written as text without a decimal point.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
