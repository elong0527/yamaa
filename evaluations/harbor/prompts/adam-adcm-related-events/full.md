Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create a RELAE dataset with one record per adverse event a relationship names
and an ADCM dataset with one record per concomitant medication record.

The RELAE dataset should contain the following columns in this order:
STUDYID, USUBJID, RELID, AESEQ, AETERM
The ADCM dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, AESEQS, AETERMS, AETERMN

RELREC ties a medication to the adverse events it was given for through a
shared relationship identifier, RELID. A relationship belongs to one subject:
the same RELID in two subjects names two different relationships. A RELREC
record names a medication when RDOMAIN is "CM" and IDVAR is "CMSEQ", and an
adverse event when RDOMAIN is "AE" and IDVAR is "AESEQ", with the sequence
number in IDVARVAL.

RELAE:
- RELAE holds one record for each adverse event a relationship names, and
  none for a related record of any other domain, such as medical history.
- RELID is the relationship, AESEQ the event's sequence number, and AETERM
  the reported term of the subject's adverse event record with that AESEQ.

ADCM:
- A medication's related adverse events are the adverse events named by the
  subject's relationship that names the medication's CMSEQ; no list includes
  related records of other domains.
- AESEQS lists the sequence numbers of the related adverse events in
  increasing numeric order, written as whole numbers and separated by a comma
  and a space (", "). Sequence numbers sort as numbers, so 2 comes before 10.
- AETERMS lists the reported terms of those events in alphabetical order,
  each term once, separated by a comma and a space (", ").
- AETERMN is the number of different terms AETERMS lists.
- When a medication is related to no adverse event, AESEQS, AETERMS and
  AETERMN have no value; AETERMN is blank, not zero.

Read the source datasets from /app/input and save the completed RELAE dataset
as /app/output/relae.csv and the completed ADCM dataset as
/app/output/adcm.csv.
