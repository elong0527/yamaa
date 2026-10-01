Following CDISC SDTM standards, use the provided TU, TR, AE, and CM
datasets to create a RELREC dataset with one row per dataset-level
relationship and per related record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

On a dataset-level row, IDVAR names the link variable (TULNKID or TRLNKID),
and RELTYPE carries ONE on the TU row and MANY on the TR row. The dataset-
level rows share RELID 1.

A record-level row has IDVAR AESEQ or CMSEQ, with IDVARVAL written as text.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
