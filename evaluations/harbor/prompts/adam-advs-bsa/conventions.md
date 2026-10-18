Following CDISC ADaM standards, use the provided ADVS dataset to create an ADVS
dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

On a body surface area record, PARAMCD is "BSA" and PARAM is "Body
Surface Area (m^2)". On a body surface area record, AVAL is the
Mosteller value, and AVAL is not rounded. DTYPE is "CALCULATION" or
has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
