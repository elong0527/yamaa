Following CDISC ADaM standards, use the provided VS and ADSL datasets
to create an ADVS dataset with one record per subject per visit per
parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AVISIT, PARAMCD, PARAM, AVAL

PARAMCD keeps each collected test code and marks each new record "BMI";
PARAM keeps each collected test name and labels each new record
"Body Mass Index (kg/m^2)".

On a BMI record AVAL is not rounded, with weight in kilograms and height
in metres.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
