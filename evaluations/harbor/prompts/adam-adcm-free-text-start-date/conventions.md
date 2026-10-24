Following CDISC ADaM standards, use the provided CM dataset to create an ADCM
dataset with one record per concomitant medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDAT, ASTDTC, ASTDT

ASTDTC is ISO 8601 text, "YYYY-MM-DD" or "YYYY-MM", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
