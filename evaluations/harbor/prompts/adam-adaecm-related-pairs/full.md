Following CDISC ADaM standards, use the provided AE, CM, and RELREC datasets to
create an ADAECM dataset with one record per adverse event and medication
related to it, per relationship that relates them.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, CMSEQ, CMTRT, RELID

Each RELREC record is one member of a subject's relationship: a record with
RDOMAIN "AE" and IDVAR "AESEQ" names an adverse event by the AESEQ in
IDVARVAL, and a record with RDOMAIN "CM" and IDVAR "CMSEQ" names a
medication by its CMSEQ.

- Every event of a relationship pairs with every medication of the same
  subject's same relationship, so a relationship of two events and two
  medications gives four records.
- A medication related to two events through two relationships appears once
  with each.
- Relationship identifiers count within a subject: two subjects'
  relationships with the same identifier are different relationships.
- AESEQ and CMSEQ are the sequence numbers of the event in AE and of the
  medication in CM, as RELREC names them; they are not renumbered.
- AETERM is the reported term of the adverse event, and CMTRT is the
  reported name of the medication.
- RELID is the identifier of the subject's relationship that relates the
  event and the medication, exactly as RELREC holds it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adaecm.csv.
