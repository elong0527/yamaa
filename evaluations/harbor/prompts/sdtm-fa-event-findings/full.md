Following CDISC SDTM standards, use the provided ODM dataset to create an FA
dataset with one record per finding about an adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTRESN, FASTRESU, FALNKID, FADTC

DOMAIN is "FA". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. Each adverse event occurrence with a
findings form gives one record for each test the form answers, and a
test with no answer on the form has no record. FATESTCD is "LOC" for
the location record, "SIZE" for the size record, and "BIOPSY" for the
biopsy record. FATEST is "Location" for a location record, "Size" for
a size record, and "Biopsied" for a biopsy record. FAOBJ is the term
of the linked adverse event, read from the IT.AE.AETERM item on the
same event occurrence. FACAT is "AE" on every record. FAORRES is the
answer as recorded on the form. FAORRESU is the size unit on "SIZE"
records and has no value on "LOC" and "BIOPSY" records. FASTRESC holds
the same value as FAORRES. FASTRESN is the measured size as a number
on "SIZE" records and has no value on "LOC" and "BIOPSY" records.
FASTRESU is the size unit on "SIZE" records and has no value on "LOC"
and "BIOPSY" records. FALNKID is the value of the IT.FA.AELNKID item
on the findings form, the link identifier shared with the linked
adverse event record. Two adverse events can share the same term, so
FALNKID, not FAOBJ, ties each finding to its specific event. FADTC is
the date the findings form was collected. An adverse event with no
findings form has no records. FASEQ numbers each subject's records
from 1 by linked event, then by test order "LOC", "SIZE", "BIOPSY",
and the collected result breaks any remaining ties.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
