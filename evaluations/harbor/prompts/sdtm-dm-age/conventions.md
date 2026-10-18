Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU

BRTHDTC is the birth date exactly as collected, as ISO 8601 text cut off
at the collected precision: a full date, a year and month, or a year
alone. AGE is the whole calendar years between the birth date and the
informed consent date: one year fewer when the birthday falls after the
consent date's month and day. A February 29 birthday has its anniversary
on February 28 in a common year.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
