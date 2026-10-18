Following CDISC ADaM standards, use the provided DM, TA, and TE datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP

TRTSEQP is TRT01P and TRT02P joined by "/" in period order, such as
"Placebo/Vitamin D3".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
