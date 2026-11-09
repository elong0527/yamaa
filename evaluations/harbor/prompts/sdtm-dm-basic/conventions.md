Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, ARM, ACTARM,
ARMNRS, COUNTRY

AGEU is "YEARS" or has no value. SEX is M for Male, F for Female, or U.
ARMNRS is "NOT ASSIGNED" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
