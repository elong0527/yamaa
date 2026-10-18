Following CDISC ADaM standards, use the provided ADPP_RAW, ADSL, and
COMPARE datasets to create an ADPP dataset with one record per subject,
parameter, and period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL, ANL01FL,
ANL02FL

ANL01FL is "Y" or has no value, and ANL02FL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
