Following CDISC ADaM standards, use the provided ADVS_RAW dataset to
create an ADVS dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

Mark a mean arterial pressure record with PARAMCD "MAP" and PARAM
"Mean Arterial Pressure (mmHg)".

On a mean arterial pressure record AVAL is not rounded.

DTYPE is CALCULATION or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
