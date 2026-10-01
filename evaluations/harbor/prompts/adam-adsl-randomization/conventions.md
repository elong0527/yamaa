Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC

RANDDTC is written as ISO 8601 text with a T between date and time, at
whole-second precision.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
