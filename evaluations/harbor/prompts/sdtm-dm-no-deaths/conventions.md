Following CDISC SDTM standards, use the provided DM_RAW dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFSTDTC, DTHDTC, DTHFL

DOMAIN is "DM". DTHFL is "Y" or has no value. The dataset keeps DTHDTC and
DTHFL even when no subject has a value for them.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
