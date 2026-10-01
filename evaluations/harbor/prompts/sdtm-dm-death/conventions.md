Following CDISC SDTM standards, use the provided DM_RAW, DS, and AE
datasets to create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL

DTHFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
