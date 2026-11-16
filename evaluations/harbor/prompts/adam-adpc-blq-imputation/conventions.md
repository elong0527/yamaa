Following CDISC ADaM standards, use the provided PC dataset to create an ADPC
dataset with one record per sample.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPTREF, ATPT, ATPTN, AVAL, AVALU,
NCAXFL

AVAL is 0 for a result below the lower limit of quantification. NCAXFL is
"Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
