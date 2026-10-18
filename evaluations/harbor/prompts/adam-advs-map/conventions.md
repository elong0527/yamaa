Following CDISC ADaM standards, use the provided ADVS dataset to create an ADVS
dataset with one record per subject, visit, and parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, VISIT, DTYPE

PARAMCD is "MAP" and PARAM is "Mean Arterial Pressure (mmHg)" on the added
record. AVAL on the added record is not rounded. DTYPE is "CALCULATION" or
has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
