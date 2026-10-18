Following CDISC SDTM standards, use the provided ODM dataset to create a DS
dataset with one record per subject for each of the two milestones: end of
treatment and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC

- DOMAIN is "DS".
- DSCAT is always "DISPOSITION EVENT".
- DSSCAT is "STUDY TREATMENT" or "STUDY".
- DSTERM is "COMPLETED" or the collected reason text.
- DSDECOD is "COMPLETED" or the collected controlled code.
- The collected controlled code is "ADVERSE EVENT" or "OTHER".

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
