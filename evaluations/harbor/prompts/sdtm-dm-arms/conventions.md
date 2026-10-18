Following CDISC SDTM standards, use the provided EX and RAND datasets to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ARMCD, ARM, ACTARMCD, ACTARM, ARMNRS, ACTARMUD

A treatment of "PLACEBO" gives ACTARMCD "PBO" and ACTARM "Placebo".
ARMNRS is "SCREEN FAILURE", "NOT ASSIGNED", or "NOT TREATED", or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
