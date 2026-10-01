Following CDISC SDTM standards, use the provided TU, TR, AE, and CM
datasets to create a RELREC dataset with one row per dataset-level
relationship and per related record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
