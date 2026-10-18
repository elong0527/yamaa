Following CDISC SDTM standards, use the provided AE, DM_RAW, and DS datasets to
create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL

DOMAIN is "DM". DTHDTC is the latest start date among the subject's
disposition records with decoded term "DEATH" that carry a start date.
When the subject has no such disposition record, DTHDTC is the latest
end date among the subject's adverse events with outcome "FATAL" that
carry an end date. When neither source records a death date for the
subject, DTHDTC has no value. DTHFL is "Y" when either source supplies
a death date for the subject, and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
