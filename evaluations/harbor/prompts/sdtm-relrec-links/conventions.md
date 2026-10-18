Following CDISC SDTM standards, use the provided AE and CM datasets to create a
RELREC dataset with one record per relationship participation.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

IDVAR is "AESEQ" or "CMSEQ". RELTYPE has no value on any row.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
