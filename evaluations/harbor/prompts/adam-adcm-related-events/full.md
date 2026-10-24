Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create a RELAE dataset with one record per adverse event that a relationship
names and an ADCM dataset with one record per concomitant medication.

The RELAE dataset should contain the following columns in this order:
STUDYID, USUBJID, RELID, AESEQ, AETERM
The ADCM dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, AESEQS, AETERMS, AETERMN

- RELAE has one record for each RELREC record whose RDOMAIN is "AE" and
  whose IDVAR is "AESEQ". RELID is that record's relationship identifier,
  AESEQ is its IDVARVAL as a number, and AETERM is the reported term of the
  subject's AE record with that AESEQ.
- A medication belongs to the relationship of the subject's RELREC record
  whose RDOMAIN is "CM", whose IDVAR is "CMSEQ" and whose IDVARVAL is the
  medication's CMSEQ. Its related adverse events are the RELAE records of
  the same subject and relationship.
- AESEQS lists the AESEQ values of the related adverse events as whole
  numbers in increasing numeric order, separated by ", " (a comma and a
  space).
- AETERMS lists the AETERM values of the related adverse events in
  alphabetical order, each different term once, separated by ", ".
- AETERMN is the number of different terms that AETERMS lists.
- AESEQS, AETERMS and AETERMN have no value, AETERMN not 0, for a medication
  with no related adverse event.
- A RELREC record that names neither an adverse event nor a medication, such
  as one for medical history, is in no list.

Read the source datasets from /app/input and save the completed RELAE dataset
as /app/output/relae.csv and the completed ADCM dataset as
/app/output/adcm.csv.
