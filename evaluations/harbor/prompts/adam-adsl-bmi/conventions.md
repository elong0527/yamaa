Following CDISC ADaM standards, use the provided ADSL dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, HEIGHTCM, WEIGHTKG, BMI, BMI_FN

BMI and BMI_FN are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
