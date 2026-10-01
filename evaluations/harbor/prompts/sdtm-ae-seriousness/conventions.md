Following CDISC SDTM standards, use the provided AE_RAW dataset to
create an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESER, AESDTH, AESLIFE, AESHOSP,
AESDISAB, AESCONG, AESMIE

AESER is Y or N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
