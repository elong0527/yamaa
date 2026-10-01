Following CDISC SDTM standards, use the provided VS dataset to create
an FA dataset with one record per qualifying temperature record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FACAT, FASCAT, FAOBJ,
FAORRES, FASTRESC, VSSTRESN

FASEQ keeps the collected sequence number. FATESTCD, FATEST, FACAT, FASCAT, and
FAOBJ are always OCCUR, Occurrence Indicator, REACTOGENICITY, SYSTEMIC, and
FEVER. FAORRES is Y or N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
