Following CDISC ADaM standards, use the provided ADSL_RAW and DS
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, COMPLFL

COMPLFL is Y or N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
