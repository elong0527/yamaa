Following CDISC ADaM standards, use the provided VS and ADSL datasets
to create an ADVS dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD keeps each collected test code and marks each new record
"BMI"; PARAM keeps each collected test name and labels each new record
"Body Mass Index (kg/m^2)".

AVAL keeps each collected result. On a BMI record it is the
weight-based index from the collected weight and the subject's
baseline height. It has no value when the baseline height is missing
or zero; the BMI record stays in place with no value. A weight record
with no result gets no BMI record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
