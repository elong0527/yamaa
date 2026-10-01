Following CDISC SDTM standards, use the provided ODM dataset to create
an FA dataset with one record per finding collected about a linked
adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

FATESTCD is LOC for the location record, SIZE for the size record, and BIOPSY
for the biopsy record, with FATEST Location, Size, and Biopsied. FACAT is
always AE. FASEQ numbers the subject's records by linked event, then by test
order LOC, SIZE, BIOPSY; the collected result breaks any remaining ties.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
