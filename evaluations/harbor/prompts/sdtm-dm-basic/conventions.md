Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SEX, AGE, ARM, ACTARM, ARMNRS

SUBJID repeats USUBJID. SEX is M for Male, F for Female, or U. AGE is in
whole years. ARMNRS is Not assigned to treatment arm or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
