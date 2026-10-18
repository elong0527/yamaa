Following CDISC SDTM standards, use the provided DM_RAW dataset to
create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RFSTDTC, SITEID, AGE, AGEU, SEX, RACE,
COUNTRY

USUBJID, SUBJID, SITEID, SEX, RACE and COUNTRY are as collected, SUBJID
keeping its leading zeros. RFSTDTC is the subject's reference start date,
with no value when the subject has none. AGE is the collected age in whole
years. AGEU is fixed to YEARS.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
