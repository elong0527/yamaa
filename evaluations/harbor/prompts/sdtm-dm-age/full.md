Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, BRTHDTC, AGE, AGEU

STUDYID is the study OID and USUBJID is the subject key, each as given in
ODM. RFICDTC is the subject's date of informed consent, the value of the
IT.DM.RFICDTC item, and BRTHDTC is the subject's birth date, the value of
the IT.DM.BRTHDT item. BRTHDTC is the birth date exactly as collected, as
ISO 8601 text cut off at the collected precision: a full date, a year and
month, or a year alone. AGE is the whole calendar years between the birth
date and the informed consent date: one year fewer when the birthday
falls after the consent date's month and day. A February 29 birthday has
its anniversary on February 28 in a common year. A birth date collected
only to the month or the year cannot give completed years, so AGE has no
value. AGEU is "YEARS" whenever AGE has a value, and has no value
otherwise. A subject with no birth date has no value for BRTHDTC, AGE,
and AGEU.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
