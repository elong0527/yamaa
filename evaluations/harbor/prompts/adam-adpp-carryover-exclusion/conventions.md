Following CDISC ADaM standards, use the provided PC and PP datasets to create
an ADPP dataset with one record per subject, period, and pharmacokinetic
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VISITNUM, VISIT, PARAMCD, PARAM, AVAL, PRECONC, PREPCMAX,
ANL01FL

PREPCMAX is not rounded. ANL01FL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
