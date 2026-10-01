Following CDISC SDTM standards, use the provided VS dataset to create
an FA dataset with one record per qualifying temperature record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FACAT, FASCAT, FAOBJ,
FAORRES, FASTRESC, VSSTRESN

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
