Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

RFICDTC is the informed consent date as collected. BRTHDTC is the birth
date exactly as collected, cut off at the collected precision. AGE is
the whole calendar years between the birth date and the consent date:
one year fewer when the birthday falls after the consent date's month
and day. A February 29 birthday has its anniversary on February 28 in a
common year. A year-month or year-only birth date cannot give completed
years, so AGE has no value, and a missing birth date leaves BRTHDTC,
AGE, and AGEU with no value. AGEU is YEARS whenever AGE is present,
and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
