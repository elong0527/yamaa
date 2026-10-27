Following CDISC SDTM standards, use the provided DM_RAW dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RFSTDTC, SITEID, AGE, AGEU, SEX, RACE,
COUNTRY

DOMAIN is "DM".

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
