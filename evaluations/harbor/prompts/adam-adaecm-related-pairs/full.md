Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create an ADAECM dataset with one record per adverse event and medication that
a relationship relates.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, CMSEQ, CMTRT, RELID

- An adverse event and a medication are related through a relationship when
  RELREC holds, for the same subject and the same RELID, a record whose
  RDOMAIN is "AE", IDVAR is "AESEQ" and IDVARVAL is the event's AESEQ, and a
  record whose RDOMAIN is "CM", IDVAR is "CMSEQ" and IDVARVAL is the
  medication's CMSEQ.
- There is one record for each event and medication a relationship relates:
  every adverse event of a relationship goes with every medication of the
  same relationship, and a medication in two relationships has a record in
  each. An event or a medication that no relationship relates to the other
  domain has no record.
- RELID values count within a subject: two subjects' relationships with the
  same RELID are different relationships.
- RELID is the relationship identifier as RELREC writes it. AESEQ and CMSEQ
  are whole numbers. AETERM is the reported term of the subject's adverse
  event with that AESEQ, and CMTRT the reported name of the subject's
  medication with that CMSEQ.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adaecm.csv.
