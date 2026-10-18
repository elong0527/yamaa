Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD is "BMI" and PARAM is "Body Mass Index (kg/m^2)" on each BMI
record. AVAL on a BMI record is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
