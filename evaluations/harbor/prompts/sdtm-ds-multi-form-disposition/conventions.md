Following CDISC SDTM standards, use the provided ODM dataset to create a DS
dataset with one record per subject per study event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSTERM, DSDECOD, DSCAT, DSSCAT, DSSTDTC

DOMAIN is "DS". DSCAT is "PROTOCOL MILESTONE" or "DISPOSITION EVENT".
DSSCAT is "INFORMED CONSENT", "RANDOMIZATION", "END OF TREATMENT" or
"END OF STUDY". DSTERM and DSDECOD take the controlled terms "INFORMED
CONSENT OBTAINED" and "RANDOMIZED", and DSTERM also takes "COMPLETED" and
"SCREEN FAILURE".

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
