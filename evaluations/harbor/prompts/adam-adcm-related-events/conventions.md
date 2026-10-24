Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create a RELAE dataset with one record per adverse event that a relationship
names and an ADCM dataset with one record per concomitant medication.

The RELAE dataset should contain the following columns in this order:
STUDYID, USUBJID, RELID, AESEQ, AETERM
The ADCM dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, AESEQS, AETERMS, AETERMN

AESEQS lists whole numbers in increasing numeric order and AETERMS lists terms
in alphabetical order, each different term once, both separated by ", ".
AESEQS, AETERMS and AETERMN have no value, AETERMN not 0, for a medication
with no related adverse event.

Read the source datasets from /app/input and save the completed RELAE dataset
as /app/output/relae.csv and the completed ADCM dataset as
/app/output/adcm.csv.
