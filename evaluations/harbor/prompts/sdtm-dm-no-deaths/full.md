Following CDISC SDTM standards, use the provided DM_RAW dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFSTDTC, DTHDTC, DTHFL

DOMAIN is "DM". RFSTDTC is the subject's reference start date, with no value
when the subject has none. DTHDTC is the date of death collected for the
subject, with no value when the subject did not die. DTHFL is "Y" when the
subject has a date of death, and has no value otherwise. The dataset keeps
DTHDTC and DTHFL even when no subject has a value for them.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
