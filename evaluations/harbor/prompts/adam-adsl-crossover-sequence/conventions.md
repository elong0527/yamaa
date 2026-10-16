Following CDISC ADaM standards, use the provided DM, TA and TE
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01P, TRT02P, TRTSEQP

TRT01P and TRT02P are Vitamin D3 or Placebo, or have no value.

TRTSEQP is TRT01P and TRT02P joined by "/" in period order, or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
