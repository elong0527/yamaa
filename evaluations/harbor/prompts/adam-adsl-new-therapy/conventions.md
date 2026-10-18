Following CDISC ADaM standards, use the provided ADSL, CM, and PR datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL

NACTDY is the study day of NACTDT counted from TRTSDT. NACTFL is "Y" or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
