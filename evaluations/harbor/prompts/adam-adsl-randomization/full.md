Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC

RANDDT is the randomization date as collected, and has no value when
no randomization date was collected.

RANDDY is the study day of randomization relative to the subject reference
start date.

RANDDTC is the collected randomization moment written as ISO 8601 text
with a T between date and time, at whole-second precision. It has no
value when no moment was collected, and it is written from what was
collected even when no randomization date was collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
