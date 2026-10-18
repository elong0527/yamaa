Following CDISC SDTM standards, use the provided AE, DM_RAW, DS, and EX
datasets to create a DM dataset with one record per enrolled subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC,
RFENDTC

DOMAIN is "DM".

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
