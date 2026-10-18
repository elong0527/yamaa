Following CDISC ADaM standards, use the provided DM and ODM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RANDDT, RANDDY, RANDDTC

RANDDY is the study day of the randomization date RANDDT, measured from
RFSTDTC, the subject's reference start date. RANDDTC is the collected
randomization moment RANDDTTM written as text at whole-second precision,
the date and time joined by T and the seconds always shown, so a moment
collected as 10:30 is written 10:30:00, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
