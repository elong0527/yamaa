Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per serious adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER

AESER is Y on every row.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
