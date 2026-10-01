Following CDISC SDTM standards, use the provided DS_RAW dataset to
create a DS dataset with one record per collected disposition record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSDECOD, DSCAT, DSDTC

DSDECOD is COMPLETED, RANDOMIZED, ADVERSE EVENT, or SCREEN FAILURE. DSCAT is
PROTOCOL MILESTONE when the outcome is RANDOMIZED, and DISPOSITION EVENT for
any other outcome.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
