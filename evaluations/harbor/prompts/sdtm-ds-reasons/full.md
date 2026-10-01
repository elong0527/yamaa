Following CDISC SDTM standards, use the provided ODM dataset to create
a DS dataset with one record per subject per milestone: end of
treatment and end of study.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSCAT, DSSCAT, DSTERM, DSDECOD, DSSTDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

DSSEQ is 1 for end of treatment and 2 for end of study. DSCAT is always
DISPOSITION EVENT. DSSCAT names the milestone: STUDY TREATMENT for the
end-of-treatment form and STUDY for the end-of-study form. DSTERM holds
the reported term: COMPLETED when the completion flag says so, else the
collected reason text. DSDECOD holds the controlled term: COMPLETED
when completed, else the collected code, either ADVERSE EVENT or OTHER.
DSSTDTC is the collected start date for that milestone. The completion
flag, reason, code, and date of one milestone are separate collected
rows, so the order of the input rows never reaches the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
