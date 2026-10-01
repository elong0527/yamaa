Following CDISC SDTM standards, use the provided ODM dataset to create
an FA dataset with one record per finding collected about a linked
adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
