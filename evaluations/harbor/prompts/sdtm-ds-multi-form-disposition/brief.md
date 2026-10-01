Following CDISC SDTM standards, use the provided ODM dataset to create
a DS dataset with one record per subject per study event: informed
consent, randomization, end of treatment, and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
