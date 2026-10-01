Following CDISC SDTM standards, use the provided ODM dataset to create
an FA dataset with one record per finding collected about a linked
adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

FATESTCD is LOC for the location record, SIZE for the size record, and
BIOPSY for the biopsy record, with FATEST Location, Size, and Biopsied.
A test with no answer on the form has no record, and an adverse event
with no findings form has no records. FAOBJ is the event term of the
linked adverse event from the same occurrence. FACAT is always AE.
FAORRES is the answer as recorded on the form. FAORRESU is the size
unit, on SIZE records only. FASTRESC repeats FAORRES. FASTRESN is the
measured size as a number, on SIZE records only. FASTRESU is the size
unit, on SIZE records only. FALNKID carries the link identifier shared
with the linked adverse event record; two events can share the same
term, so FAOBJ alone does not distinguish them. FADTC is the date the
findings form was collected. FASEQ numbers the subject's records by
linked event, then by test order LOC, SIZE, BIOPSY; the collected result
breaks any remaining ties.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
