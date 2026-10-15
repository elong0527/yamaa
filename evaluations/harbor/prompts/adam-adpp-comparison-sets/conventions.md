Following CDISC ADaM standards, use the provided ADPP_RAW, ADSL and
COMPARE datasets to create an ADPP dataset with one record per PK
parameter per subject per period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, APERIOD, TRTA, TRTSEQP, AVAL, ANL01FL,
ANL02FL

ANL01FL belongs to comparison 1 and ANL02FL to comparison 2: the flag
numbered nn belongs to the comparison whose COMPNO is nn. Each flag is Y
or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
