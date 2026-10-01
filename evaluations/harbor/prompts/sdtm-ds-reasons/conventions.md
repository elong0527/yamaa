Following CDISC SDTM standards, use the provided ODM dataset to create
a DS dataset with one record per subject per milestone: end of
treatment and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC

DSCAT is always DISPOSITION EVENT. DSSCAT names the milestone: STUDY
TREATMENT for the end-of-treatment form and STUDY for the end-of-study form.
DSTERM holds the reported term: COMPLETED or the collected reason text.
DSDECOD holds the controlled term: COMPLETED, ADVERSE EVENT, or OTHER.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
