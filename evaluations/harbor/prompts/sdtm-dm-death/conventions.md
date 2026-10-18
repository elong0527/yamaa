Following CDISC SDTM standards, use the provided AE, DM_RAW, and DS datasets to
create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL

DOMAIN is "DM". DTHFL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
