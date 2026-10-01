Following CDISC SDTM standards, use the provided DM and ODM datasets to
create an SV dataset with one record per subject per visit attended.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC,
SVENDTC, SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
