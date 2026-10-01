Following CDISC SDTM standards, use the provided DM_RAW, EX, DS, and
AE datasets to create a DM dataset with one record per enrolled
subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC,
RFENDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
