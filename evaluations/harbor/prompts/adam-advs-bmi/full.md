Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per subject per visit per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD keeps each collected test code ("HEIGHT" and "WEIGHT") and marks
each BMI record "BMI". PARAM keeps each collected test name and labels
each BMI record "Body Mass Index (kg/m^2)". AVAL keeps each collected
result. A BMI record is added for each collected weight record with a
result, and a weight record without a result gets no BMI record. On a
BMI record, AVAL is the collected weight in kilograms divided by the
square of the subject's baseline height in metres. AVAL has no value on
a BMI record when the subject's baseline height is missing or zero, and
the BMI record stays in place. AVAL on a BMI record is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
