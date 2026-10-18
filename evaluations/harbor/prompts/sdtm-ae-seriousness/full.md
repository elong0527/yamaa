Following CDISC SDTM standards, use the provided AE_RAW dataset to create an AE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESER, AESDTH, AESLIFE, AESHOSP,
AESDISAB, AESCONG, AESMIE

AESER is "Y" when any of the seriousness criteria AESDTH, AESLIFE, AESHOSP,
AESDISAB, AESCONG, or AESMIE is "Y". AESER is "N" when no criterion is "Y",
including when every criterion has no value. A criterion flag with no value
stays empty rather than becoming "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
