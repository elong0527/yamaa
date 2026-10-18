Following CDISC SDTM standards, use the provided ODM dataset to create an FA
dataset with one record per finding about an adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC

DOMAIN is "FA". FATESTCD is "LOC" for the location record, "SIZE" for
the size record, and "BIOPSY" for the biopsy record. FATEST is
"Location" for a location record, "Size" for a size record, and
"Biopsied" for a biopsy record. FACAT is "AE" on every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
