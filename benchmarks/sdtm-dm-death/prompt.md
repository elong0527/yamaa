Following CDISC SDTM standards, use the provided DM_RAW, DS, and AE
datasets to create a DM dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DTHDTC, DTHFL

DTHDTC is the latest disposition death date, or the latest fatal
adverse event end date when disposition records no death. A disposition
death is a record with DSDECOD DEATH; a fatal event is one with AEOUT
FATAL. DTHDTC has no value when neither source records a death. DTHFL
is Y when either source supplies a death date, and has no value
otherwise. When the sources record different death dates, the
disposition date wins. An undated death or fatal event cannot displace
a dated one.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
