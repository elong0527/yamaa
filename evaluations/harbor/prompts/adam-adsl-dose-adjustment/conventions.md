Following CDISC ADaM standards, use the provided ADSL_RAW, EX, EC, and
FA datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, DOSADJFL

DOSADJFL is Y, N, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
