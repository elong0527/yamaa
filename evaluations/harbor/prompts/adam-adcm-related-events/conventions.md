Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create a RELAE dataset with one record per adverse event a relationship names
and an ADCM dataset with one record per concomitant medication record.

The RELAE dataset should contain the following columns in this order:
STUDYID, USUBJID, RELID, AESEQ, AETERM
The ADCM dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, AESEQS, AETERMS, AETERMN

AESEQS lists sequence numbers in increasing numeric order, written as whole
numbers, and AETERMS lists terms in alphabetical order, each term once, both
separated by a comma and a space (", "). AESEQS, AETERMS and AETERMN may have
no value; AETERMN is then blank, not zero.

Read the source datasets from /app/input and save the completed RELAE dataset
as /app/output/relae.csv and the completed ADCM dataset as
/app/output/adcm.csv.
