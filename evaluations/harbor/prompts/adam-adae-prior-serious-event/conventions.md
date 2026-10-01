Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, AESER, PRIOR_SAEFL, PRIOR_SAEDECOD,
PRIOR_SAESEQ, FIRST_SAEDECOD, FIRST_SAESEQ, PREV_AEDECOD

PRIOR_SAEFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
